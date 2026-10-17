"""Closed21→20 retained-observation projection for the password addition.

preserve() is a historical projection, NOT a standalone freshness validator.
The current runtime entry must validate the entire current report, Cargo inputs,
native closure and password evidence BEFORE invoking it. Historical projections
never pretend their old snapshots describe current files.
"""
import copy
import hashlib
import json
from pathlib import Path

from cargo_authoring import contained_file, unique_object
from hashing_provenance import NATIVE_SOURCES as HASH_NATIVE
from password_provenance import NATIVE_SOURCES as PASSWORD_NATIVE

ROOT = Path(__file__).resolve().parents[1]
PATH = ROOT / "corpus/password-coin-migration.json"
PIN = "3be6e6bdf4c580b56b295f4d3cfaeca4d8eaf837d9e80b0772217348ff263847"
COMMIT = "3a61698390e80e0b9131dd1ec878b52ff48afaed"
BEFORE_REPORT = "7866a312bfe5c05c2c0c7543b8c52eada14f1dbab793f46dfd4f70f0d3fbe791"
BEFORE_NORMALIZED = "dbfde7a6f609e82a3aa8047ef0cbb95ec5be4c43e5481af55e7c9acc2c627ca3"
TRANSITIONS = ["crates/chialisp-oxide-corpus/src/lib.rs", "crates/chialisp-oxide-corpus/src/native.rs"]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load(payload=None):
    payload = PATH.read_bytes() if payload is None else payload
    require(digest(payload) == PIN, "Reviewed password migration snapshot changed")
    migration = json.loads(payload, object_pairs_hook=unique_object)
    require(type(migration["schema_version"]) is int and migration["schema_version"] == 1
            and migration["added_slug"] == "password-coin", "Unsupported password transition")
    require(migration["reviewed_before"] == {"commit": COMMIT, "report_sha256": BEFORE_REPORT,
            "normalized_report_sha256": BEFORE_NORMALIZED}, "Changed committed20 baseline identity")
    require(len(migration["preserved_slugs"]) == 20 and len(set(migration["preserved_slugs"])) == 20,
            "Malformed preserved20 identities")
    transitions = migration["native_source_transitions"]
    require([row["path"] for row in transitions] == TRANSITIONS, "Unknown native-source normalization")
    for row in transitions:
        require(set(row) == {"path", "affected_slugs", "before", "after"}
                and row["affected_slugs"] == ["sha256tree", "singleton-launcher"], "Changed normalization targets")
        for phase in ("before", "after"):
            require(set(row[phase]) == {"source", "sha256"}
                    and digest(row[phase]["source"].encode()) == row[phase]["sha256"], "Native source snapshot/hash mismatch")
        require(row["before"] != row["after"], "No-op native source normalization")
    table = migration["authoring_inputs"]
    for phase, count in (("before", 53), ("after", 57)):
        rows = table[phase]
        require(len(rows) == count and [r["path"] for r in rows] == sorted({r["path"] for r in rows}),
                "Malformed authoring input snapshot")
    before, after = ({r["path"]: r["sha256"] for r in table[key]} for key in ("before", "after"))
    changes = {"changed_existing": sorted(p for p in before.keys() & after.keys() if before[p] != after[p]),
               "added": sorted(after.keys() - before.keys()), "removed": sorted(before.keys() - after.keys())}
    require(changes == migration["authoring_changes"], "Authoring delta differs from reviewed source snapshots")
    require(changes["changed_existing"] == TRANSITIONS + ["puzzles/authoring.json", "puzzles/lib.rs"]
            and changes["added"] == ["crates/chialisp-oxide-corpus/src/password.rs", "crates/chialisp-oxide-corpus/src/password/ledger.rs",
                "puzzles/password-coin/generated-cases.json", "puzzles/password-coin/puzzle.rs"] and not changes["removed"],
            "Unknown changed/added/removed Cargo input")
    for row in transitions:
        require(before[row["path"]] == row["before"]["sha256"] and after[row["path"]] == row["after"]["sha256"],
                "Native snapshots are disconnected from declared Cargo inputs")
    ir = migration["raw_ir_baseline"]
    require(ir["commit"] == COMMIT and len(ir["entries"]) == 20
            and {r["entry"] for r in ir["entries"]} == {
                f"chialisp_oxide_puzzles::{slug.replace('-', '_')}::puzzle" for slug in migration["preserved_slugs"]},
            "Malformed reviewed rawIR identities")
    return migration


def current_native_sources(report):
    """Check closed actual paths/bytes/authoring before any report projection.

    Paths come from reviewed constants and the pinned source table, never from
    an unchecked report-supplied filename.
    """
    migration = load()
    require([p["slug"] for p in report["puzzles"]] == migration["preserved_slugs"] + ["password-coin"],
            "Password transition permits only its exact appended21st fixture")
    require(report["authoring"]["sources"] == migration["authoring_inputs"]["after"],
            "Current Cargo source hashes differ from reviewed password registration")
    inputs = {row["path"]: row["sha256"] for row in migration["authoring_inputs"]["after"]}
    for path, expected in inputs.items():
        require(digest(contained_file(ROOT, path).read_bytes()) == expected,
                "Current reviewed Cargo source is stale: " + path)
    for puzzle in report["puzzles"]:
        paths = PASSWORD_NATIVE if puzzle["slug"] == "password-coin" else HASH_NATIVE.get(puzzle["slug"], [])
        rows = puzzle.get("native_sources", [])
        require(isinstance(rows, list) and [row.get("path") for row in rows] == paths,
                "Current native-source closure has stale/missing/duplicate/unknown paths")
        for row, path in zip(rows, paths):
            require(set(row) == {"path", "sha256"} and row["sha256"] == inputs[path]
                    and row["sha256"] == digest(contained_file(ROOT, path).read_bytes()),
                    "Current native source row/bytes/authoring mismatch: " + path)


def preserve(report):
    """Historical-only projection; caller must run current validation first."""
    migration = load()
    require([p["slug"] for p in report["puzzles"]] == migration["preserved_slugs"] + ["password-coin"],
            "Password transition permits only the exact appended fixture/order")
    result = copy.deepcopy(report)
    result["puzzles"].pop()
    for transition in migration["native_source_transitions"]:
        seen = []
        for puzzle in result["puzzles"]:
            for row in puzzle.get("native_sources", []):
                if row["path"] == transition["path"]:
                    require(row == {"path": transition["path"], "sha256": transition["after"]["sha256"]},
                            "Unreviewed native-source metadata transition")
                    row["sha256"] = transition["before"]["sha256"]
                    seen.append(puzzle["slug"])
        require(seen == transition["affected_slugs"], "Missing/duplicate/unexpected transitioned native row")
    normalized = {key: value for key, value in result.items() if key != "authoring"}
    require(digest(canonical(normalized)) == BEFORE_NORMALIZED,
            "Password changed a retained20 source, artifact, cost, output, typed failure, order, coverage or ledger observation")
    return result


def verify_ir_bundle(manifest_path, report):
    """Compare an explicitly supplied complete21-export bundle; never extract.

    This is byte retention/manifest consistency, not compiler authentication or
    proof of Cargo provenance. The caller must obtain the bundle from a fresh
    coordinated extraction. Paths are fixed entry-hash leaf names.
    """
    from cargo_bundle import _identity
    from password_evidence import artifact_bytes
    migration = load()
    manifest_path = manifest_path.resolve()
    require(manifest_path.name == "manifest.json" and manifest_path.stat().st_size <= 4 * 1024 * 1024,
            "Invalid/oversized supplied Cargo manifest")
    manifest = json.loads(manifest_path.read_bytes(), object_pairs_hook=unique_object)
    require(type(manifest.get("schema_version")) is int and manifest["schema_version"] == 1
            and manifest.get("package") == "chialisp-oxide-puzzles"
            and manifest.get("status") == "wip_prototype_not_ready_for_use", "Unsupported supplied Cargo bundle")
    # The producer orders immutable entry files by their hashed leaf names,
    # rather than Rust display names or the corpus's source registration order.
    names = sorted((f"chialisp_oxide_puzzles::{slug.replace('-', '_')}::puzzle"
                   for slug in migration["preserved_slugs"] + ["password-coin"]),
                   key=lambda name: digest(name.encode()))
    entries = manifest["entries"]
    require(isinstance(entries, list) and [entry["entry"] for entry in entries] == names,
            "Cargo bundle must have exactly the reviewed21 export identities")
    old = {row["entry"]: row["sha256"] for row in migration["raw_ir_baseline"]["entries"]}
    puzzles = {f"chialisp_oxide_puzzles::{p['slug'].replace('-', '_')}::puzzle": p for p in report["puzzles"]}
    required_files = {"manifest.json"}
    mode = None
    for entry in entries:
        require(set(entry) == {"entry", "artifact", "clvm", "ir"}, "Unexpected Cargo entry fields")
        stem = digest(entry["entry"].encode())
        paths = {extension: f"{stem}.{extension}" for extension in ("ir.json", "clvm.hex", "clvm.txt", "artifact.json")}
        require(entry["ir"] == paths["ir.json"] and entry["clvm"] == paths["clvm.hex"], "Cargo entry path is not its exact hashed leaf")
        payloads = {}
        for extension, name in paths.items():
            path = manifest_path.parent / name
            require(path.is_file() and not path.is_symlink() and path.stat().st_size <= 4 * 1024 * 1024,
                    "Missing/symlinked/oversized Cargo artifact file")
            payloads[extension] = path.read_bytes()
            required_files.add(name)
        ir = json.loads(payloads["ir.json"], object_pairs_hook=unique_object)
        require(_identity(ir) == entry["entry"], "IR has a different export identity")
        if entry["entry"] in old:
            require(digest(payloads["ir.json"]) == old[entry["entry"]], "An original20 rawIR byte string changed")
        artifact = entry["artifact"]
        binary = artifact_bytes(artifact)
        require(artifact["backend"] == "rust_mir", "Cargo entry is not actual Rust MIR mode")
        if mode is None:
            mode = artifact["optimize_lir"]
        require(artifact["optimize_lir"] is mode, "Mixed compiler modes in one Cargo bundle")
        matching = [a for a in puzzles[entry["entry"]]["artifacts"]
                    if a["backend"] == "rust_mir" and a["optimize_lir"] is mode]
        require(len(matching) == 1 and artifact == matching[0], "Cargo artifact differs from recorded actual mode")
        require(payloads["clvm.hex"] == (binary.hex() + "\n").encode()
                and payloads["clvm.txt"] == (artifact["disassembly"] + "\n").encode()
                and json.loads(payloads["artifact.json"], object_pairs_hook=unique_object) == artifact,
                "Cargo retained files disagree with their manifest artifact")
    require({p.name for p in manifest_path.parent.iterdir()} == required_files,
            "Missing/unknown files in supplied immutable bundle")
    return {"compared_old_ir": 20, "checked_current_exports": 21, "optimize_lir": mode}


def controls(report):
    preserve(report)
    count = 0
    for mutation in ("old_source", "old_artifact", "old_cost", "old_output", "old_ledger", "old_order",
                     "missing_old", "missing_new", "duplicate_new", "unknown_new", "new_order", "native_hash", "native_duplicate"):
        bad = copy.deepcopy(report)
        first = bad["puzzles"][0]
        if mutation == "old_source": first["source"] += "\n"
        elif mutation == "old_artifact": first["artifacts"][0]["clvm_hex"] += "80"
        elif mutation == "old_cost": first["cases"][0]["runs"][0]["execution"]["cost"] += 1
        elif mutation == "old_output": first["cases"][0]["native"]["output_hex"] = "80"
        elif mutation == "old_ledger": first["simulator"][0]["name"] += " changed"
        elif mutation == "old_order": bad["puzzles"][0:2] = reversed(bad["puzzles"][0:2])
        elif mutation == "missing_old": bad["puzzles"].pop(0)
        elif mutation == "missing_new": bad["puzzles"].pop()
        elif mutation == "duplicate_new": bad["puzzles"].append(copy.deepcopy(bad["puzzles"][-1]))
        elif mutation == "unknown_new": bad["puzzles"][-1]["slug"] = "unknown-password-puzzle"
        elif mutation == "new_order": bad["puzzles"][-2:] = reversed(bad["puzzles"][-2:])
        else:
            puzzle = next(p for p in bad["puzzles"] if p["slug"] == "sha256tree")
            if mutation == "native_hash": puzzle["native_sources"][2]["sha256"] = "0" * 64
            else: puzzle["native_sources"].append(copy.deepcopy(puzzle["native_sources"][2]))
        require(canonical(bad) != canonical(report), "No-op password preservation corruption: " + mutation)
        try:
            preserve(bad)
        except (ValueError, KeyError):
            count += 1
        else:
            raise ValueError("Password preservation corruption passed: " + mutation)
    for mutation in ("stale", "missing", "duplicate", "unknown", "authoring"):
        bad = copy.deepcopy(report)
        puzzle = next(p for p in bad["puzzles"] if p["slug"] == "sha256tree")
        if mutation == "stale": puzzle["native_sources"][2]["sha256"] = "0" * 64
        elif mutation == "missing": puzzle["native_sources"].pop()
        elif mutation == "duplicate": puzzle["native_sources"].append(copy.deepcopy(puzzle["native_sources"][0]))
        elif mutation == "unknown": puzzle["native_sources"][0]["path"] = "unknown/native.rs"
        else: bad["authoring"]["sources"][0]["sha256"] = "0" * 64
        try:
            current_native_sources(bad)
        except (ValueError, KeyError):
            count += 1
        else:
            raise ValueError("Current native freshness corruption passed: " + mutation)
    for key in ("scope", "reviewed_before", "preserved_slugs", "added_slug", "native_source_transitions", "raw_ir_baseline", "authoring_inputs"):
        changed = copy.deepcopy(load())
        if key == "scope": changed[key] += " altered"
        elif key == "added_slug": changed[key] = "unknown"
        elif key == "preserved_slugs": changed[key].reverse()
        else: changed[key] = {}
        try:
            load(canonical(changed))
        except ValueError:
            count += 1
        else:
            raise ValueError("Altered password snapshot passed: " + key)
    return count
