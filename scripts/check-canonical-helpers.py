#!/usr/bin/env python3
"""Check one pinned helper adaptation and its finite three-language evidence.

This verifies source identities, the visible wrapper and recorded outcomes. It is
not a compiler/termination proof or promotion of the enclosing library/macro.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import runpy

from cargo_authoring import contained_file
from vm_failures import PROTOCOL, check_execution, check_native_outcome

ROOT = Path(__file__).resolve().parents[1]
PIN = "11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4"
TREE = "5be895a68c298617e3e376c7c060009b3e1144d8"
BASE = f"corpus/upstream/chia-gaming/{PIN}"
SOURCE = f"{BASE}/clsp/std/last.clinc"
LICENSE = f"{BASE}/LICENSE.MD"
SPAN = f"{BASE}/last_inner.clinc"
COMMIT = f"{BASE}/commit.git"
TREE_MANIFEST = f"corpus/upstream/published-puzzles/Chia-Network--chia-gaming/{PIN}/tree.json"
TREE_SHA256 = "7c545d9100d060ec52722e2047ee2cc6cef33b4aa82feb2a04a55caec46a9b67"
MANIFEST = "puzzles/last-list-element/puzzle.json"
REFERENCE = "puzzles/last-list-element/reference.clsp"
RUST = "puzzles/last-list-element/puzzle.rs"
RUE = "puzzles/last-list-element/puzzle.rue"
REPORT = ROOT / "corpus/validation.json"
PINS = {
    SOURCE: ("aeb7cd23e4b062fa35e8d47f332f6c6c25cfed883734043967b6ba59caca7f19", "8e65cf562b481b2a0a8eca8fc9575c74649d28aa"),
    LICENSE: ("d2e8598a416f31a166c5ec34aef7568111a4eb903d7958551290f8fcd3e9c22c", "75f455f1363bf4e1ededc01bf932051d18a4c61f"),
}
SPAN_SHA256 = "67984c12a663e4522c28d8343eefe6aee7e79100c326282822ddb823cabaa6dc"
REFERENCES = [
    f"https://github.com/Chia-Network/chia-gaming/blob/{PIN}/clsp/std/last.clinc#L8-L13",
    f"https://github.com/Chia-Network/chia-gaming/blob/{PIN}/LICENSE.MD",
    f"https://github.com/Chia-Network/chia-gaming/commit/{PIN}",
]
PREFIX = b"; Raw-environment module adapter; the six-line upstream helper below is verbatim.\n(mod ENV\n"
SUFFIX = b"  (last_inner ENV)\n)\n"
BUILDS = {("rust_mir", False), ("rust_mir", True), ("chialisp", True), ("rue_source", False), ("rue_source", True)}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_source(path):
    return contained_file(ROOT, path).read_bytes()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def git_object(kind, value):
    return hashlib.sha1(kind.encode() + b" " + str(len(value)).encode() + b"\0" + value).hexdigest()


def sources(reader=read_source):
    for path, (sha256, blob) in PINS.items():
        data = reader(path)
        require(digest(data) == sha256 and git_object("blob", data) == blob, f"Pinned source/license changed: {path}")
    commit = reader(COMMIT)
    require(git_object("commit", commit) == PIN and commit.startswith(f"tree {TREE}\n".encode()), "Pinned Git commit/tree identity changed")
    span = b"".join(reader(SOURCE).splitlines(keepends=True)[7:13])
    require(span == reader(SPAN) and digest(span) == SPAN_SHA256, "Exact helper extraction changed")
    require(reader(REFERENCE) == PREFIX + span + SUFFIX, "The helper was changed, guarded, or wrapped with a different environment contract")
    require(digest(reader(TREE_MANIFEST)) == TREE_SHA256, "Pinned complete Git tree snapshot changed")
    tree = json.loads(reader(TREE_MANIFEST))
    require(tree["repository"] == "Chia-Network/chia-gaming" and tree["revision"] == PIN and tree["root_tree_sha1"] == TREE, "Wrong repository/commit tree")
    # Reuse the complete Git Merkle reconstruction used by the source catalog.
    spec = importlib.util.spec_from_file_location("published_tree_validator", ROOT / "corpus/validate_published_puzzles.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.check_tree(tree)
    entries = {entry["path"]: entry for entry in tree["entries"]}
    for upstream, local in [("clsp/std/last.clinc", SOURCE), ("LICENSE.MD", LICENSE)]:
        require(entries[upstream]["kind"] == "blob" and entries[upstream]["mode"] == "100644"
                and entries[upstream]["git_sha1"] == PINS[local][1]
                and entries[upstream]["size_bytes"] == len(reader(local)), "Source/license not bound to the pinned tree")
    manifest = json.loads(reader(MANIFEST))
    require(manifest["slug"] == "last-list-element" and manifest["kind"] == "pure_function"
            and manifest["environment"] == "raw" and manifest.get("reference_mode", "module") == "module", "Changed helper/raw-module contract")
    p = manifest["provenance"]
    require(p["classification"] == "canonical_helper_port", "Helper must not be promoted as a whole canonical puzzle")
    require(p["references"] == REFERENCES, "Changed pinned source/license/commit links")
    require(p["upstream"] == {
        "repository": "https://github.com/Chia-Network/chia-gaming", "revision": PIN,
        "source_path": "clsp/std/last.clinc", "snapshot": SOURCE, "source_sha256": PINS[SOURCE][0],
        "git_blob_sha1": PINS[SOURCE][1], "root_tree_sha1": TREE,
    }, "False upstream helper identity")
    for key, expected in {"helper": "last_inner", "start_line": 8, "end_line": 13, "snapshot": SPAN, "sha256": SPAN_SHA256}.items():
        require(p["source_span"][key] == expected, "False helper span identity")
    for key, expected in {"spdx": "Apache-2.0", "source_path": "LICENSE.MD", "snapshot": LICENSE, "sha256": PINS[LICENSE][0], "git_blob_sha1": PINS[LICENSE][1]}.items():
        require(p["license"][key] == expected, "False license identity")
    for key, expected in {"snapshot": COMMIT, "sha256": digest(commit), "git_commit_sha1": PIN, "root_tree_sha1": TREE}.items():
        require(p["commit"][key] == expected, "False commit binding")
    for key, expected in {"manifest": TREE_MANIFEST, "sha256": TREE_SHA256, "root_tree_sha1": TREE}.items():
        require(p["tree"][key] == expected, "False tree binding")
    for key, expected in {"kind": "raw_environment_module_wrapper", "reference": REFERENCE, "reference_sha256": digest(reader(REFERENCE)), "helper_span_sha256": SPAN_SHA256}.items():
        require(p["source_adaptation"][key] == expected, "False wrapper adaptation identity")
    require("upstream_artifact" not in p and "module_identity_obligation" not in p, "Include helper has no canonical compiled-module claim")
    return manifest


# Independent data-level expected relation; the codec is shared with the other
# canonical-source gate, not with the compiler or native Rust policy.
codec = runpy.run_path(str(ROOT / "scripts/check-canonical-factories.py"))


def expected(solution):
    cursor = codec["parse"](solution)
    while isinstance(cursor, tuple):
        first, rest = cursor
        if rest == b"":
            return {"status": "accepted", "output_hex": codec["serialize"](first).hex()}
        cursor = rest
    return {"status": "rejected"}


def validate(report, reader=read_source):
    manifest = sources(reader)
    require(report.get("vm_failure_protocol") == PROTOCOL, "Missing typed VM failure protocol")
    matches = [p for p in report["puzzles"] if p["slug"] == manifest["slug"]]
    require(len(matches) == 1, "Missing/duplicate helper evidence")
    puzzle = matches[0]
    require(puzzle.get("reference_mode", "module") == "module", "Helper report must use the original module adapter")
    for field in ("title", "kind", "environment", "security_scope", "provenance"):
        require(puzzle[field] == manifest[field], f"Changed helper claim/contract: {field}")
    require(puzzle["validated"] is True and puzzle["source"] == reader(RUST).decode()
            and puzzle["reference"] == reader(REFERENCE).decode(), "Stale or unvalidated helper sources")
    rue = puzzle["rue"]
    require(rue["path"] == RUE and rue["source"] == reader(RUE).decode()
            and rue["sha256"] == digest(reader(RUE)), "Stale Rue helper source")
    require(not puzzle["simulator"] and not puzzle["simulator_comparisons"]
            and "module_identity" not in puzzle and "factory_checks" not in puzzle,
            "Helper evidence must not claim ledger/factory/module identity")
    require([(c["label"], c["solution"]) for c in puzzle["cases"]]
            == [(c["label"], c["solution"]) for c in manifest["cases"]], "Missing/changed helper vectors")
    require({(a["backend"], a["optimize_lir"]) for a in puzzle["artifacts"]} == BUILDS
            and len(puzzle["artifacts"]) == 5, "Missing helper compiler mode")
    accepted = rejected = 0
    for case in puzzle["cases"]:
        wanted = expected(case["solution"])
        check_native_outcome(case["native"], case["label"])
        require(case["native"] == wanted, f"Independent last_inner expectation differs: {case['label']}")
        require({(r["backend"], r["optimize_lir"]) for r in case["runs"]} == BUILDS
                and len(case["runs"]) == 5, "Incomplete helper executions")
        for run in case["runs"]:
            require(check_execution(run["execution"], case["label"]) == wanted, "Helper execution differs from the original data-level contract")
        if wanted["status"] == "accepted": accepted += 1
        else: rejected += 1
    require(type(puzzle["generated_cases"]) is int and puzzle["generated_cases"] == 2103,
            "Expected raw2,059 +44 targeted returned-element/list/tail comparisons")
    return len(puzzle["cases"]), accepted, rejected


def self_test(report=None):
    mutations = [
        ("source", {SOURCE: b"changed source"}), ("license", {LICENSE: b"changed license"}),
        ("span", {SPAN: read_source(SPAN) + b"\n"}), ("commit", {COMMIT: read_source(COMMIT) + b"\n"}),
        ("tree", {TREE_MANIFEST: read_source(TREE_MANIFEST) + b"\n"}),
        ("wrapper", {REFERENCE: read_source(REFERENCE).replace(b"(last_inner ENV)", b"(last_inner (f ENV))")}),
    ]
    manifest = sources()
    for field in ("classification", "blob", "span", "license", "wrapper", "references", "reference_mode"):
        bad = copy.deepcopy(manifest)
        if field == "classification": bad["provenance"]["classification"] = "canonical_puzzle_port"
        elif field == "blob": bad["provenance"]["upstream"]["git_blob_sha1"] = "0" * 40
        elif field == "span": bad["provenance"]["source_span"]["start_line"] = 1
        elif field == "license": bad["provenance"]["license"]["git_blob_sha1"] = "0" * 40
        elif field == "wrapper": bad["provenance"]["source_adaptation"]["kind"] = "none"
        elif field == "references": bad["provenance"]["references"][0] = "https://github.com/Chia-Network/chia-gaming/blob/main/clsp/std/last.clinc#L8-L13"
        else: bad["reference_mode"] = "expression"
        mutations.append((f"metadata/{field}", {MANIFEST: json.dumps(bad).encode()}))
    for label, overrides in mutations:
        try: sources(lambda path: overrides[path] if path in overrides else read_source(path))
        except (ValueError, KeyError): continue
        raise ValueError(f"Provenance mutation was accepted: {label}")
    controls = len(mutations)
    for solution, wanted in [("()", {"status": "rejected"}), ("(1 . 0x00)", {"status": "rejected"}),
                             ("(1 . 0x0000)", {"status": "rejected"}), ("(())", {"status": "accepted", "output_hex": "80"}),
                             ("(1 2 3)", {"status": "accepted", "output_hex": "03"}), ("(1 (2 . 3))", {"status": "accepted", "output_hex": "ff0203"})]:
        require(expected(solution) == wanted, "Helper checker confused nil, bytes, or list-final element")
    if report is not None:
        validate(report)
        for mutation in ("missing_case", "native", "vm", "missing_failure", "resource", "unknown", "ledger", "identity", "scope", "coverage", "reference_mode"):
            bad = copy.deepcopy(report)
            p = next(p for p in bad["puzzles"] if p["slug"] == "last-list-element")
            if mutation == "missing_case": p["cases"].pop()
            elif mutation == "native": p["cases"][0]["native"] = {"status": "accepted", "output_hex": "80"}
            elif mutation == "vm": p["cases"][0]["runs"][0]["execution"]["outcome"] = {"status": "accepted", "output_hex": "80"}
            elif mutation == "missing_failure": del p["cases"][0]["runs"][0]["execution"]["failure"]
            elif mutation == "resource": p["cases"][0]["runs"][0]["execution"]["failure"] = {"kind": "resource", "code": "cost_exceeded"}
            elif mutation == "unknown": p["cases"][0]["runs"][0]["execution"]["failure"]["code"] = "unknown_code"
            elif mutation == "ledger": p["simulator"] = [{"name": "invented ledger admission"}]
            elif mutation == "identity": p["module_identity"] = {"upstream_hex": "01"}
            elif mutation == "scope": p["provenance"]["classification"] = "canonical_puzzle_port"
            elif mutation == "reference_mode": p["reference_mode"] = "expression"
            else: p["generated_cases"] -= 1
            try: validate(bad)
            except (ValueError, KeyError): controls += 1; continue
            raise ValueError(f"False helper evidence was accepted: {mutation}")
    return controls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=REPORT)
    parser.add_argument("--sources-only", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        report = None if args.sources_only else json.loads(args.report.read_text())
        if report is None: sources()
        else:
            cases, accepted, rejected = validate(report)
            print(f"PASS: exact last_inner helper span + explicit raw module adapter; {cases} finite vectors ({accepted} accepted/{rejected} rejected), five builds; no whole-puzzle/module identity claim")
        if args.self_test:
            scope = "source/provenance (execution not checked)" if report is None else "provenance/executed-evidence"
            print(f"PASS: {self_test(report)} helper {scope} mutation controls")
        elif report is None: print("PASS: pinned helper/license/commit/tree/span and explicit wrapper sources (execution not checked)")
    except (ValueError, KeyError, TypeError, OSError) as error:
        parser.exit(1, f"Canonical helper check failed: {error}\n")


if __name__ == "__main__":
    main()
