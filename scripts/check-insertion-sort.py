#!/usr/bin/env python3
"""Pinned insertion-helper provenance and finite independent tree expectations.

The oracle uses iterative insertion/rebuilding, not Rust, Rue or compiled code.
No compiler preservation, whole credential checker or termination proof is claimed.
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
SLUG = "insertion-sort-by-keys"
PIN = "cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8"
TREE = "e131409ed769ef4def743d4ebbcf859b40fcec2d"
REPOSITORY = "https://github.com/Chia-Network/chia_puzzles"
BASE = f"corpus/upstream/chia-puzzles/{PIN}"
ORIGINAL = f"{BASE}/puzzles/vc_puzzles/cr_puzzles/flag_proofs_checker.clsp"
UTILITY = f"{BASE}/puzzles/utility_macros.clib"
SPAN = f"{BASE}/insertion_sort_by_keys.clinc"
OR_SPAN = f"{BASE}/or_macro.clinc"
LICENSE = f"{BASE}/LICENSE"
COMMIT = f"{BASE}/commit.git"
TREE_MANIFEST = f"corpus/upstream/published-puzzles/Chia-Network--chia_puzzles/{PIN}/tree.json"
TREE_SHA256 = "9900f500c6397560e20046c5bf4d4016a651f9000d6893512317ce6067c7842d"
DIRECTORY = f"puzzles/{SLUG}"
MANIFEST, RUST, REFERENCE, RUE, GENERATED = (f"{DIRECTORY}/{name}" for name in
    ("puzzle.json", "puzzle.rs", "reference.clsp", "puzzle.rue", "generated-cases.json"))
SPAN_SHA256 = "a2479b0ece2e7a0af47a3144e99552069199de48c571136befff12cc4f529952"
OR_SHA256 = "84b12b50be9c2ae79b2445054a9dc2b0cc1a04df55bf53b4dc3c147e1db98945"
PINS = {
    ORIGINAL: ("53c4eddf9d269f6d2f72bf7eb6903c70bb5bcdaa99f85c0a35ad76550a4a51e4", "b83f8a8a0def7be4b3c467308eeef1dd64ebaa6c"),
    UTILITY: ("f58331cdceeb52133d780bc3adc9b4d1154277510f39be9d4fa78a4d9acd2d4a", "271a4ba5136a260f1eca0df3ab8b247dc8dfb7c2"),
    LICENSE: ("7db0cf1060026bf2a544ea29cf9a96fba7d3781f93a1d06d6fee18251f1c207a", "37b84d60dd8a34d854a41e94c34eaf587648ac6c"),
}
DEPENDENCIES = ["crates/chialisp-oxide/src/lib.rs", "crates/chialisp-oxide/src/owned_tree.rs", UTILITY, GENERATED]
MODE_ORDER = [("rust_mir", False), ("rust_mir", True), ("chialisp", True), ("rue_source", False), ("rue_source", True)]
HEADER = b"; Strict two-tree-argument adapter; both upstream spans below are verbatim.\n(mod ENV\n"
BETWEEN = b"\n; Published helper from flag_proofs_checker.clsp lines 15-23.\n"
FOOTER = b"""  (if (l ENV)
      (if (l (r ENV))
          (if (r (r ENV))
              (x)
              (insertion_sort_by_keys (f ENV) (f (r ENV)))
          )
          (x)
      )
      (x)
  )
)
"""
codec = runpy.run_path(str(ROOT / "scripts/check-canonical-factories.py"))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_source(path):
    return contained_file(ROOT, path).read_bytes()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def git_object(kind, value):
    return hashlib.sha1(f"{kind} {len(value)}\0".encode() + value).hexdigest()


def list_tree(items, tail=b""):
    for value in reversed(items):
        tail = value, tail
    return tail


def display(value):
    if isinstance(value, bytes):
        return "0x" + value.hex() if value else "()"
    items = []
    while isinstance(value, tuple):
        items.append(display(value[0]))
        value = value[1]
    return "(" + " ".join(items) + ("" if value == b"" else " . " + display(value)) + ")"


class ProgramReject(Exception):
    pass


def key(entry):
    if not isinstance(entry, tuple) or not isinstance(entry[0], bytes):
        raise ProgramReject()
    return entry[0]


def expected_tree(environment, *, compare=None, zero_nil=False, eager=False, loose=False, drop_values=False):
    """Insert each pending head using a cursor and an explicit rebuilt prefix.

    No input-size bound is inserted into the policy; the independent oracle's
    work guard raises a host error, never a policy rejection.
    """
    compare = compare or (lambda left, right: left > right)
    nil = lambda value: isinstance(value, bytes) and (not any(value) if zero_nil else value == b"")
    try:
        if not isinstance(environment, tuple) or not isinstance(environment[1], tuple):
            raise ProgramReject()
        if not loose and environment[1][1] != b"":
            raise ProgramReject()
        sorted_tree, pending = environment[0], environment[1][0]
        if eager:
            for tree in (sorted_tree, pending):
                while not nil(tree):
                    if not isinstance(tree, tuple): raise ProgramReject()
                    key(tree[0])
                    tree = tree[1]
        work = 0
        while not nil(pending):
            if not isinstance(pending, tuple): raise ProgramReject()
            entry, pending = pending
            if drop_values and isinstance(entry, tuple): entry = (entry[0], b"")
            cursor, prefix = sorted_tree, []
            while not nil(cursor):
                work += 1
                require(work <= 1_000_000, "Independent oracle work bound exceeded")
                if not isinstance(cursor, tuple): raise ProgramReject()
                if compare(key(entry), key(cursor[0])): break
                prefix.append(cursor[0])
                cursor = cursor[1]
            sorted_tree = entry, cursor
            for head in reversed(prefix): sorted_tree = head, sorted_tree
        return {"status": "accepted", "output_hex": codec["serialize"](sorted_tree).hex()}
    except ProgramReject:
        return {"status": "rejected"}


def expected(solution, **options):
    return expected_tree(codec["parse"](solution), **options)


def named_cases():
    cases = [
        ("both lists nil", "(() ())"),
        ("empty insertion preserves nonempty atom", "(1 ())"),
        ("empty insertion preserves one zero byte", "(0x00 ())"),
        ("empty insertion preserves padded zero bytes", "(0x0000 ())"),
        ("empty insertion preserves arbitrary improper sorted tree", "(((1 . 2) . 0x00) ())"),
        ("empty insertion does not inspect a pair key", "((((1 . 2) . 9)) ())"),
        ("empty sorted accepts one nil entry without reading a key", "(() (()))"),
        ("empty sorted accepts one nonempty atom entry", "(() (0x00))"),
        ("empty sorted accepts one pair key without inspecting it", "(() (((1 . 2) . 9)))"),
        ("one ordinary key and value", "(() ((0x61 . 7)))"),
        ("higher inserted key goes first", "(((0x61 . 7)) ((0x62 . 8)))"),
        ("lower inserted key goes after existing entry", "(((0x62 . 7)) ((0x61 . 8)))"),
        ("equal keys retain existing before inserted payload", "(((0x61 . 7)) ((0x61 . 8)))"),
        ("equal pending keys retain their payload order", "(() ((0x61 . 7) (0x61 . 8) (0x61 . 9)))"),
        ("unsorted first input is not silently presorted", "(((0x61 . 7) (0x63 . 9)) ((0x62 . 8)))"),
        ("unsorted input can place a new equal key before an existing equal key", "(((0x61 . 1) (0x62 . 2)) ((0x62 . 3)))"),
        ("multiple pending entries insert into existing list", "(((0x63 . 3) (0x61 . 1)) ((0x62 . 2) (0x64 . 4)))"),
        ("proper-prefix key is smaller", "(((0x61 . 1)) ((0x6100 . 2)))"),
        ("empty key is smaller than zero byte", "(((( ) . 1)) ((0x00 . 2)))"),
        ("numeric zero padding has distinct byte order", "(((0x00 . 1)) ((0x0000 . 2)))"),
        ("numeric one padding is lexically smaller", "(((0x01 . 1)) ((0x0001 . 2)))"),
        ("high-bit bytes use unsigned element order", "(((0x7f . 1)) ((0x80 . 2)))"),
        ("invalid UTF8 key bytes remain unchanged", "(((0xfffe . 1)) ((0xffff . 2)))"),
        ("arbitrary pair payload and program-looking data survive", "(((0x61 . (1 . 2))) ((0x62 . (1 . (8 . 9)))))"),
        ("higher key leaves improper sorted tail untouched", "(((0x61 . 1) . 0x00) ((0x62 . 2)))"),
        ("lower key reaches and rejects improper sorted tail", "(((0x62 . 1) . 0x00) ((0x61 . 2)))"),
        ("equal key reaches and rejects improper sorted tail", "(((0x61 . 1) . 0x0000) ((0x61 . 2)))"),
        ("nonempty sorted atom rejects when insertion needs a key", "(0x00 ((0x61 . 2)))"),
        ("one malformed existing head rejects when compared", "((1) ((0x61 . 2)))"),
        ("malformed pending head rejects against nonempty sorted", "(((0x61 . 1)) (2))"),
        ("pair-valued pending key rejects when compared", "(((0x61 . 1)) (((1 . 2) . 9)))"),
        ("a second entry forces inspection of earlier malformed head", "(() (1 (0x61 . 2)))"),
        ("nonempty insertion atom rejects even if all bytes are zero", "(() 0x00)"),
        ("padded-zero insertion atom is not nil", "(() 0x0000)"),
        ("improper pending tail rejects after one entry", "(() ((0x61 . 1) . 0x00))"),
        ("missing both outer arguments", "()"),
        ("whole environment atom", "0x00"),
        ("missing insertion argument", "(())"),
        ("extra outer argument", "(() () ())"),
        ("nonempty outer zero tail", "(() () . 0x00)"),
        ("pair outer tail is an extra argument", "(() () . (1 . 2))"),
    ]
    large = b"B" * 8192
    for label, other in [("equal 8192-byte keys preserve tie payloads", large),
                         ("last byte decides 8192-byte key order", large[:-1] + b"C")]:
        cases.append((label, display(list_tree([list_tree([(large, b"\x01")]), list_tree([(other, b"\x02")])]))))
    return [{"label": label, "solution": solution} for label, solution in cases]


def generated_cases():
    rows, seen = [], set()
    def add(label, environment):
        text = display(environment)
        binary = codec["serialize"](environment)
        if binary not in seen:
            seen.add(binary)
            rows.append({"label": label, "solution": text})
    keys = [b"", b"\x00", b"\x01", b"\x7f", b"\x80", b"\xff", b"\x00\x00", b"\x00\x01"]
    for i, left in enumerate(keys):
        for j, right in enumerate(keys):
            add(f"key cross-product {i}/{j}", list_tree([list_tree([(left, b"\x01")]), list_tree([(right, b"\x02")])]))
            add(f"pending tie cross-product {i}/{j}", list_tree([list_tree([(left, b"\x01")]), list_tree([(right, b"\x02"), (left, b"\x03")])]))
            add(f"unsorted input cross-product {i}/{j}", list_tree([list_tree([(right, b"\x01"), (left, b"\x02")]), list_tree([(left, b"\x03")])]))
    heads = [b"", b"\x00", b"\x01", (b"a", b"x"), ((b"a", b"b"), b"v")]
    sorted_values = [b"", b"\x00", list_tree([(b"a", b"v")]), list_tree([(b"a", b"v")], b"\x00")]
    for i, first in enumerate(sorted_values):
        for j, head in enumerate(heads):
            for tail in [b"", b"\x00", b"\x00\x00"]:
                add(f"lazy/shape first {i}; entry {j}; tail {tail.hex()}", list_tree([first, (head, tail)]))
    for width in [0, 1, 2, 31, 32, 63, 64, 255, 256]:
        left = b"B" * width
        for label, right in [("equal", left), ("prefix", left + b"\x00"), ("different final", left[:-1] + b"C" if left else b"\x00")]:
            add(f"key width {width}; {label}", list_tree([list_tree([(left, (b"x", b"y"))]), list_tree([(right, b"\x00\x01")])]))
    for text in ["()", "0x00", "(())", "(() () ())", "(() () . 0x00)", "(() () . 0x0000)", "(1 . 2)"]:
        add("malformed outer " + text, codec["parse"](text))
    return rows


def sources(reader=read_source):
    for path, (sha, blob) in PINS.items():
        data = reader(path)
        require(digest(data) == sha and git_object("blob", data) == blob, f"Changed pinned source/license: {path}")
    commit = reader(COMMIT)
    require(digest(commit) == "68502b042abaad03f35dd59c0fce3d7d1ce000c1302ccf50c762a955b2ec4215"
            and git_object("commit", commit) == PIN and commit.startswith(f"tree {TREE}\n".encode()), "Changed pinned commit/tree identity")
    require(digest(reader(TREE_MANIFEST)) == TREE_SHA256, "Changed complete tree snapshot")
    tree = json.loads(reader(TREE_MANIFEST))
    require(tree["repository"] == "Chia-Network/chia_puzzles" and tree["revision"] == PIN
            and tree["root_tree_sha1"] == TREE, "Wrong source repository/tree")
    spec = importlib.util.spec_from_file_location("sort_published_tree_validator", ROOT / "corpus/validate_published_puzzles.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.check_tree(tree)
    entries = {entry["path"]: entry for entry in tree["entries"]}
    for local in PINS:
        path = local.removeprefix(BASE + "/")
        require(entries[path]["kind"] == "blob" and entries[path]["mode"] == "100644"
                and entries[path]["git_sha1"] == PINS[local][1]
                and entries[path]["size_bytes"] == len(reader(local)), "Source/license absent from pinned Git tree")
    span = b"".join(reader(ORIGINAL).splitlines(keepends=True)[14:23])
    macro = b"".join(reader(UTILITY).splitlines(keepends=True)[20:28])
    require(span == reader(SPAN) and digest(span) == SPAN_SHA256, "Changed exact insertion-helper span")
    require(macro == reader(OR_SPAN) and digest(macro) == OR_SHA256, "Changed exact lazy-or macro span")
    require(reader(REFERENCE) == HEADER + macro + BETWEEN + span + FOOTER, "Changed helper, macro or strict visible adapter")
    manifest = json.loads(reader(MANIFEST))
    require(manifest["slug"] == SLUG and manifest["kind"] == "pure_function"
            and manifest.get("environment", "strict") == "strict"
            and manifest.get("reference_mode", "module") == "module", "Changed helper input/output contract")
    require(manifest["source_dependencies"] == DEPENDENCIES, "Changed declared helper dependencies")
    require(manifest["cases"] == named_cases(), "Changed independent named-vector coverage")
    payload = (json.dumps(generated_cases(), indent=2) + "\n").encode()
    require(len(payload) < 128 * 1024 and reader(GENERATED) == payload, "Changed deterministic bounded generated input file")
    provenance = manifest["provenance"]
    require(provenance["classification"] == "canonical_helper_port", "Helper must not be promoted to whole canonical puzzle")
    require(provenance["references"] == [
        f"{REPOSITORY}/blob/{PIN}/puzzles/vc_puzzles/cr_puzzles/flag_proofs_checker.clsp#L15-L23",
        f"{REPOSITORY}/blob/{PIN}/puzzles/utility_macros.clib#L21-L28", f"{REPOSITORY}/blob/{PIN}/LICENSE", f"{REPOSITORY}/commit/{PIN}"], "Changed pinned reference URLs")
    require(provenance["upstream"] == {"repository": REPOSITORY, "revision": PIN,
        "source_path": "puzzles/vc_puzzles/cr_puzzles/flag_proofs_checker.clsp", "snapshot": ORIGINAL,
        "source_sha256": PINS[ORIGINAL][0], "git_blob_sha1": PINS[ORIGINAL][1], "root_tree_sha1": TREE}, "False upstream identity")
    groups = {
        "source_span": {"helper": "insertion_sort_by_keys", "start_line": 15, "end_line": 23, "snapshot": SPAN, "sha256": SPAN_SHA256},
        "license": {"spdx": "Apache-2.0", "source_path": "LICENSE", "snapshot": LICENSE, "sha256": PINS[LICENSE][0], "git_blob_sha1": PINS[LICENSE][1]},
        "commit": {"snapshot": COMMIT, "sha256": digest(commit), "git_commit_sha1": PIN, "root_tree_sha1": TREE},
        "tree": {"manifest": TREE_MANIFEST, "sha256": TREE_SHA256, "root_tree_sha1": TREE},
        "source_adaptation": {"kind": "strict_two_argument_module_wrapper", "reference": REFERENCE,
            "reference_sha256": digest(reader(REFERENCE)), "helper_span_sha256": SPAN_SHA256, "macro_span_sha256": OR_SHA256},
    }
    for name, required in groups.items():
        for field, value in required.items():
            require(provenance[name][field] == value, f"False helper {name}/{field} binding")
    macros = provenance["supporting_macros"]
    require(len(macros) == 1, "Missing/extra original macro dependencies")
    for field, value in {"name": "or", "source_path": "puzzles/utility_macros.clib", "snapshot": UTILITY,
                        "source_sha256": PINS[UTILITY][0], "git_blob_sha1": PINS[UTILITY][1],
                        "start_line": 21, "end_line": 28, "span_snapshot": OR_SPAN, "span_sha256": OR_SHA256}.items():
        require(macros[0][field] == value, f"False lazy-or macro identity: {field}")
    require("upstream_artifact" not in provenance and "module_identity_obligation" not in provenance,
            "An extracted helper/adapter has no published module-identity claim")
    return manifest


def fits_host_budget(value):
    pending, count, total = [(value, 0)], 0, 0
    while pending:
        tree, depth = pending.pop()
        count += 1
        if depth > 128 or count > 16_384: return False
        if isinstance(tree, tuple): pending.extend([(tree[0], depth + 1), (tree[1], depth + 1)])
        else:
            total += len(tree)
            if total > 1_048_576: return False
    return True


def validate(report, reader=read_source):
    manifest = sources(reader)
    require(report.get("vm_failure_protocol") == PROTOCOL, "Missing typed VM-failure protocol")
    matches = [puzzle for puzzle in report["puzzles"] if puzzle["slug"] == SLUG]
    require(len(matches) == 1, "Missing/duplicate insertion-helper evidence")
    puzzle = matches[0]
    for field in ("title", "kind", "security_scope", "provenance"):
        require(puzzle[field] == manifest[field], f"Changed insertion-helper claim: {field}")
    require(puzzle.get("environment", "strict") == "strict" and puzzle.get("reference_mode", "module") == "module"
            and puzzle["validated"] is True, "Wrong/unvalidated helper environment/reference contract")
    require(puzzle["source"] == reader(RUST).decode() and puzzle["reference"] == reader(REFERENCE).decode(), "Stale Rust/Chialisp sources")
    rue = puzzle["rue"]
    require(rue["path"] == RUE and rue["source"] == reader(RUE).decode() and rue["sha256"] == digest(reader(RUE))
            and rue["dependencies"] == [], "Stale Rue source/dependencies")
    require([row["path"] for row in puzzle["source_dependencies"]] == DEPENDENCIES, "Missing/reordered helper inputs")
    for dependency in puzzle["source_dependencies"]:
        source = reader(dependency["path"])
        require(dependency["source"] == source.decode() and dependency["sha256"] == digest(source), "Stale recorded helper dependency")
    require(puzzle["simulator"] == [] and puzzle["simulator_comparisons"] == []
            and "module_identity" not in puzzle and "factory_checks" not in puzzle, "Helper must not gain ledger/factory/module-identity claims")
    require([(c["label"], c["solution"]) for c in puzzle["cases"]] == [(c["label"], c["solution"]) for c in manifest["cases"]], "Missing/changed named executions")
    require([(a["backend"], a["optimize_lir"]) for a in puzzle["artifacts"]] == MODE_ORDER, "Missing/reordered actual compiler modes")
    for artifact in puzzle["artifacts"]:
        require(type(artifact["optimize_lir"]) is bool and type(artifact["bytes"]) is int, "Malformed artifact mode/size")
        binary = bytes.fromhex(artifact["clvm_hex"])
        require(len(binary) == artifact["bytes"] and codec["tree_hash"](codec["deserialize"](binary)).hex() == artifact["tree_hash"], "Artifact bytes/tree hash disagree")
    accepted = rejected = 0
    for case in puzzle["cases"]:
        desired = expected(case["solution"])
        require(check_native_outcome(case["native"], case["label"]) == desired, f"Native helper differs from independent insertion relation: {case['label']}")
        require([(r["backend"], r["optimize_lir"]) for r in case["runs"]] == MODE_ORDER, "Missing/reordered case execution modes")
        for run in case["runs"]:
            require(type(run["optimize_lir"]) is bool and check_execution(run["execution"], case["label"]) == desired,
                    "VM helper differs from independent insertion relation")
        accepted += desired["status"] == "accepted"
        rejected += desired["status"] == "rejected"
    require(type(puzzle["generated_cases"]) is int and puzzle["generated_cases"] == len(generated_cases()), "Changed generated-input count")
    return [(SLUG, len(puzzle["cases"]), accepted, rejected, len(generated_cases()))]


def self_test(report=None):
    manifest = sources()
    overrides = [
        ("original", {ORIGINAL: read_source(ORIGINAL) + b"\n"}), ("macro source", {UTILITY: read_source(UTILITY) + b"\n"}),
        ("license", {LICENSE: read_source(LICENSE) + b"\n"}), ("commit", {COMMIT: read_source(COMMIT) + b"\n"}),
        ("tree", {TREE_MANIFEST: read_source(TREE_MANIFEST) + b"\n"}), ("helper span", {SPAN: read_source(SPAN) + b"\n"}),
        ("macro span", {OR_SPAN: read_source(OR_SPAN) + b"\n"}), ("adapter", {REFERENCE: read_source(REFERENCE).replace(b"(r (r ENV))", b"()", 1)}),
        ("generated", {GENERATED: b"[]\n"}), ("missing original or", {REFERENCE: read_source(REFERENCE).replace(read_source(OR_SPAN), b"", 1)}),
    ]
    for mutation in ("classification", "blob", "license", "span", "macro", "macro_blob", "reference", "abi", "mode", "dependencies", "cases", "module_claim"):
        bad = copy.deepcopy(manifest)
        if mutation == "classification": bad["provenance"]["classification"] = "canonical_puzzle_port"
        elif mutation == "blob": bad["provenance"]["upstream"]["git_blob_sha1"] = "0" * 40
        elif mutation == "license": bad["provenance"]["license"]["git_blob_sha1"] = "0" * 40
        elif mutation == "span": bad["provenance"]["source_span"]["start_line"] = 14
        elif mutation == "macro": bad["provenance"]["supporting_macros"] = []
        elif mutation == "macro_blob": bad["provenance"]["supporting_macros"][0]["git_blob_sha1"] = "0" * 40
        elif mutation == "reference": bad["provenance"]["references"][0] = REPOSITORY + "/blob/main/puzzle.clsp"
        elif mutation == "abi": bad["environment"] = "raw"
        elif mutation == "mode": bad["reference_mode"] = "expression"
        elif mutation == "dependencies": bad["source_dependencies"] = []
        elif mutation == "cases": bad["cases"].pop()
        else: bad["provenance"]["module_identity_obligation"] = "claims canonical module"
        overrides.append((mutation, {MANIFEST: json.dumps(bad).encode()}))
    for label, changes in overrides:
        require(any(read_source(path) != value for path, value in changes.items()), f"No-op source mutation: {label}")
        try: sources(lambda path: changes.get(path, read_source(path)))
        except (ValueError, KeyError): pass
        else: raise ValueError(f"Source/provenance corruption passed: {label}")
    rows = named_cases() + generated_cases()
    for text, output in [("(0x00 ())", "00"), ("(() (0x00))", "ff0080"),
                         ("(() 0x00)", None), ("(() () . 0x00)", None),
                         ("(((0x61 . 1)) ((0x61 . 2)))", "ffff6101ffff610280"),
                         ("(((0x61 . 1) . 0x00) ((0x62 . 2)))", "ffff6202ffff610100"),
                         ("(((0x61 . 1) (0x62 . 2)) ((0x62 . 3)))", "ffff6203ffff6101ffff620280")]:
        desired = {"status": "rejected"} if output is None else {"status": "accepted", "output_hex": output}
        require(expected(text) == desired, "Independent relation confused lazy keys, nil, ties or improper tails")
    mutations = [
        ("numeric comparison", {"compare": lambda a, b: int.from_bytes(a, "big", signed=True) > int.from_bytes(b, "big", signed=True)}),
        ("length-first ordering", {"compare": lambda a, b: (len(a), a) > (len(b), b)}),
        ("non-strict tie comparison", {"compare": lambda a, b: a >= b}),
        ("reversed ordering", {"compare": lambda a, b: a < b}),
        ("normalize zero padding", {"compare": lambda a, b: a.lstrip(b"\x00") > b.lstrip(b"\x00")}),
        ("zero atoms treated as nil", {"zero_nil": True}), ("eager full validation", {"eager": True}),
        ("drop payloads", {"drop_values": True}), ("loose outer ABI", {"loose": True}),
    ]
    for name, options in mutations:
        require(any(expected(row["solution"], **options) != expected(row["solution"]) for row in rows), f"No witness detects oracle mutation: {name}")
    for row in rows:
        require(fits_host_budget(codec["parse"](row["solution"])), "Input test data exceeds whole-tree host budget")
        result = expected(row["solution"])
        if result["status"] == "accepted":
            require(fits_host_budget(codec["deserialize"](bytes.fromhex(result["output_hex"]))), "Successful test result exceeds host budget")
    count = len(overrides) + len(mutations)
    if report is not None:
        validate(report)
        for mutation in ("missing_case", "native", "vm", "resource", "missing_failure", "unknown_failure", "ledger", "module", "reference_mode", "environment", "generated", "modes"):
            bad = copy.deepcopy(report)
            puzzle = next(p for p in bad["puzzles"] if p["slug"] == SLUG)
            case = next(c for c in puzzle["cases"] if c["native"]["status"] == "rejected")
            execution = case["runs"][0]["execution"]
            if mutation == "missing_case": puzzle["cases"].pop()
            elif mutation == "native": case["native"] = {"status": "accepted", "output_hex": "80"}
            elif mutation == "vm": execution["outcome"] = {"status": "accepted", "output_hex": "80"}
            elif mutation == "resource": execution["failure"] = {"kind": "resource", "code": "cost_exceeded"}
            elif mutation == "missing_failure": del execution["failure"]
            elif mutation == "unknown_failure": execution["failure"]["code"] = "unknown_failure"
            elif mutation == "ledger": puzzle["simulator"] = [{"name": "invented spend"}]
            elif mutation == "module": puzzle["module_identity"] = {"upstream_hex": "01"}
            elif mutation == "reference_mode": puzzle["reference_mode"] = "expression"
            elif mutation == "environment": puzzle["environment"] = "raw"
            elif mutation == "generated": puzzle["generated_cases"] -= 1
            else: puzzle["artifacts"].reverse()
            require(bad != report, f"No-op report mutation: {mutation}")
            try: validate(bad)
            except (ValueError, KeyError): pass
            else: raise ValueError(f"Recorded-evidence corruption passed: {mutation}")
            count += 1
    return count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--source-only", action="store_true", help="check provenance/oracles without claiming executed report evidence")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--write-vectors", action="store_true", help="reproduce only this fixture's deterministic generated JSON")
    args = parser.parse_args()
    if args.write_vectors:
        (ROOT / GENERATED).write_text(json.dumps(generated_cases(), indent=2) + "\n")
        print(f"Wrote {len(generated_cases())} bounded input rows; no execution evidence generated")
        return
    report = None if args.source_only else json.loads(args.report.read_text())
    if report is None: sources()
    else: print("Recorded finite evidence:", validate(report))
    if args.self_test: print(f"PASS: {self_test(report)} provenance/relation/record corruption controls")
    if report is None: print("PASS: pinned sources + independent finite data relation only; no Rust/VM execution claim")


if __name__ == "__main__":
    main()
