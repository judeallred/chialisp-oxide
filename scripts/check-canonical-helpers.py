#!/usr/bin/env python3
"""Check pinned helper adapters and their finite three-language evidence.

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
import unicodedata

from cargo_authoring import contained_file
from vm_failures import PROTOCOL, check_execution, check_native_outcome

ROOT = Path(__file__).resolve().parents[1]
PIN = "11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4"
TREE = "5be895a68c298617e3e376c7c060009b3e1144d8"
BASE = f"corpus/upstream/chia-gaming/{PIN}"
SOURCE = f"{BASE}/clsp/std/last.clinc"
ATOM_SOURCE = f"{BASE}/clsp/std/relops.clinc"
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
    ATOM_SOURCE: ("24a50e28a6a695035e66fad1b81555b03bb509e960a35bd53235a7982a229843", "1c9bcd8c3112c3884fd68b3907acae5f84975be8"),
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


def common_sources(reader=read_source):
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
    for upstream, local in [("clsp/std/last.clinc", SOURCE), ("clsp/std/relops.clinc", ATOM_SOURCE), ("LICENSE.MD", LICENSE)]:
        require(entries[upstream]["kind"] == "blob" and entries[upstream]["mode"] == "100644"
                and entries[upstream]["git_sha1"] == PINS[local][1]
                and entries[upstream]["size_bytes"] == len(reader(local)), "Source/license not bound to the pinned tree")
    return commit


STRICT_SUFFIX = b"""  (if (l ENV)
      (if (l (r ENV))
          (if (r (r ENV))
              (x)
              (prefix (f ENV) (f (r ENV)))
          )
          (x)
      )
      (x)
  )
)
"""
PROFILES = {
    "last-list-element": {"source": SOURCE, "helper": "last_inner", "start": 8, "end": 13, "span": SPAN,
        "sha256": SPAN_SHA256, "environment": "raw", "adaptation": "raw_environment_module_wrapper",
        "header": PREFIX, "footer": SUFFIX, "named": 16,
        "dependencies": ["crates/chialisp-oxide/src/lib.rs", "crates/chialisp-oxide/src/tree.rs"]},
    "prefix-list": {"source": SOURCE, "helper": "prefix", "start": 1, "end": 6, "span": f"{BASE}/prefix.clinc",
        "sha256": "48caa279faddbe3a2bfaedac0a34b3ff814bf5b69ebb0708292120fac8d7e227",
        "environment": "strict", "adaptation": "strict_two_argument_module_wrapper",
        "header": b"; Strict two-argument adapter; the original six-line helper is verbatim.\n(mod ENV\n",
        "footer": STRICT_SUFFIX, "named": 69,
        "dependencies": ["crates/chialisp-oxide/src/lib.rs", "crates/chialisp-oxide/src/owned_tree.rs", "puzzles/prefix-list/generated-cases.json"]},
    "atom-not-equal": {"source": ATOM_SOURCE, "helper": "!=", "start": 4, "end": 4, "span": f"{BASE}/not_equal.clinc",
        "sha256": "c7f16a16596a1dc64b1ad3038e07b4935f56a89fa14570233da6fb157515310f",
        "environment": "strict", "adaptation": "strict_two_atom_argument_module_wrapper",
        "header": b"; Strict two-argument adapter around the exact published inline helper.\n(mod ENV\n",
        # The original final source line has no LF. This separator belongs to
        # the adapter, not the verbatim extracted helper span.
        "footer": b"\n  (if (l ENV)\n      (if (l (r ENV))\n          (if (r (r ENV)) (x) (!= (f ENV) (f (r ENV))))\n          (x))\n      (x))\n)\n",
        "named": 32, "dependencies": ["puzzles/atom-not-equal/generated-cases.json"]},
}
GENERATED = "puzzles/prefix-list/generated-cases.json"
GENERATED_ATOMS = "puzzles/atom-not-equal/generated-cases.json"
MODE_ORDER = [("rust_mir", False), ("rust_mir", True), ("chialisp", True), ("rue_source", False), ("rue_source", True)]
codec = runpy.run_path(str(ROOT / "scripts/check-canonical-factories.py"))


def prefix_generated_cases():
    """Deterministic bounded inputs; no bound is inserted into compiled policy."""
    rows, seen = [], set()

    def add(label, solution):
        if solution not in seen:
            seen.add(solution)
            rows.append({"label": label, "solution": solution})

    def spine(length, head, tail):
        for _ in range(length):
            tail = f"({head} . {tail})"
        return tail

    tails = ["()", "0x00", "0x0000", "0x0001", "-1", "(8)", "(1 . 2)", "((8) . 0x0000)"]
    for length in [0, 1, 2, 8, 32, 64]:
        for ending in ["()", "0x00", "0x0000", "0x0001", "-1"]:
            for head in ["()", "0x0001", "(1 . 2)", "(8)"]:
                left = spine(length, head, ending)
                for tail in tails:
                    add(f"left-spine {length}; ending {ending}; head {head}; arbitrary P {tail}", f"({left} {tail})")
    # The largest successful output has64+64 pair links; the strict input adds
    # at most two outer argument links, not128 links on one input branch.
    for left_length in [0, 1, 8, 32, 64]:
        left = spine(left_length, "0x0001", "()")
        for tail_length in [0, 1, 8, 32, 64]:
            for ending in ["()", "0x0000"]:
                tail = spine(tail_length, "0x00", ending)
                add(f"output boundary {left_length}+{tail_length}; tail ending {ending}", f"({left} {tail})")
    for width in [0, 1, 31, 32, 63, 64, 255, 256, 4096, 8192]:
        atom = "0x" + "00" * width
        for label, solution in [("head", f"(({atom}) (8))"), ("P", f"((1 2) {atom})"),
                                ("improper L ending", f"((1 . {atom}) ())")]:
            add(f"exact returned bytes {label}; atom width {width}", solution)
    add("different heads preserve order", "((1 2 0x0001 (8)) ((3 . 4) . 0x0000))")
    atoms = ["()", "1", "-1", "0x00", "0x0000", "0x0001", "(8)", "(1 . 2)"]
    for first in atoms:
        add("malformed whole atom or tree", first)
        add("one argument", f"({first})")
        for second in atoms:
            for ending in ["0x00", "0x0000", "0x0001", "(3)"]:
                add("nonempty strict argument tail", f"({first} {second} . {ending})")
    return rows


def atom_generated_cases():
    """Byte and length boundaries; these bounds are not compiled policy guards."""
    def atom(value):
        return "0x" + value.hex() if value else "()"

    def env(left, right):
        return f"({atom(left)} {atom(right)})"

    rows, seen = [], set()

    def add(label, solution):
        key = codec["serialize"](codec["parse"](solution))
        if key not in seen:
            seen.add(key)
            rows.append({"label": label, "solution": solution})

    small = [b""] + [bytes([value]) for value in
                     (0, 1, 2, 3, 15, 16, 31, 32, 63, 64, 65, 66, 127, 128, 129, 254, 255)]
    small += [bytes.fromhex(value) for value in
              ("0000", "0001", "0002", "007f", "0080", "00ff", "ff00", "ff7f", "ffff",
               "000001", "010000", "000100", "610062", "616263", "c3a9", "65cc81", "fffe",
               "f09f92a1", "42424242", "00000000", "01010101", "80808080")]
    require(len(small) == len(set(small)) == 40, "Changed atom cross-product coverage")
    for left in small:
        for right in small:
            add(f"small atom {atom(left)} versus {atom(right)}", env(left, right))
    widths = (0, 1, 2, 3, 7, 8, 15, 16, 31, 32, 33, 47, 48, 63, 64, 65, 127, 128,
              255, 256, 257, 511, 512, 1023, 1024, 4095, 4096, 8191, 8192)
    for width in widths:
        patterns = [b"B" * width]
        if width <= 256:
            patterns += [b"\x00" * width, bytes(index % 251 for index in range(width))]
        for index, base in enumerate(patterns):
            add(f"equal atoms width {width}, pattern {index}", env(base, base))
            for position in sorted({0, width // 2, width - 1} if width else set()):
                changed = base[:position] + bytes([base[position] ^ 1]) + base[position + 1:]
                add(f"different byte {position} at width {width}, pattern {index}", env(base, changed))
                add(f"reversed different byte {position} at width {width}, pattern {index}", env(changed, base))
            for extended in [base + b"\x00", b"\x00" + base]:
                if len(extended) <= 8192:
                    add(f"different atom lengths {len(base)} and {len(extended)}, pattern {index}", env(base, extended))
                    add(f"reversed atom lengths {len(extended)} and {len(base)}, pattern {index}", env(extended, base))
    for left in small[:16]:
        for right in small[:8]:
            add(f"left pair with head {atom(left)} and atom {atom(right)}", f"(({atom(left)} . 1) {atom(right)})")
            add(f"right pair with head {atom(right)} and atom {atom(left)}", f"({atom(left)} ({atom(right)} . 1))")
    for value in ["()", "0x00", "0x0000", "1", "(())", "(1)", "((1 . 2))", "(1 . 2)", "(() () ())", "(1 2 3 4)"]:
        add("malformed environment " + value, value)
    for tail in ["0x00", "0x0000", "0x0001", "-1", "(3)", "(3 . 4)"]:
        for left in small[:8]:
            add(f"non-nil argument ending {tail} after {atom(left)}", f"({atom(left)} {atom(left)} . {tail})")
    return rows


def sources(reader=read_source):
    commit = common_sources(reader)
    manifests = {}
    for slug, profile in PROFILES.items():
        path = f"puzzles/{slug}/puzzle.json"
        reference = f"puzzles/{slug}/reference.clsp"
        span = b"".join(reader(profile["source"]).splitlines(keepends=True)[profile["start"] - 1:profile["end"]])
        require(span == reader(profile["span"]) and digest(span) == profile["sha256"], f"Changed exact span: {slug}")
        require(reader(reference) == profile["header"] + span + profile["footer"], f"Changed visible helper wrapper: {slug}")
        manifest = json.loads(reader(path))
        require(manifest["slug"] == slug and manifest["kind"] == "pure_function"
                and manifest.get("environment", "strict") == profile["environment"] and manifest.get("reference_mode", "module") == "module",
                f"Changed helper/ABI/module contract: {slug}")
        require(len(manifest["cases"]) == profile["named"], f"Changed named helper coverage: {slug}")
        require(manifest["source_dependencies"] == profile["dependencies"], f"Missing/changed declared helper inputs: {slug}")
        p = manifest["provenance"]
        require(p["classification"] == "canonical_helper_port", "Helper must not be promoted as a whole canonical puzzle")
        source_path = profile["source"].removeprefix(BASE + "/")
        anchor = f"L{profile['start']}" + (f"-L{profile['end']}" if profile['end'] != profile['start'] else "")
        references = [f"https://github.com/Chia-Network/chia-gaming/blob/{PIN}/{source_path}#{anchor}",
                      f"https://github.com/Chia-Network/chia-gaming/blob/{PIN}/LICENSE.MD",
                      f"https://github.com/Chia-Network/chia-gaming/commit/{PIN}"]
        require(p["references"] == references, "Changed pinned source/license/commit links")
        require(p["upstream"] == {"repository": "https://github.com/Chia-Network/chia-gaming", "revision": PIN,
            "source_path": source_path, "snapshot": profile["source"], "source_sha256": PINS[profile["source"]][0],
            "git_blob_sha1": PINS[profile["source"]][1], "root_tree_sha1": TREE}, "False upstream helper identity")
        expected_metadata = {
            "source_span": {"helper": profile["helper"], "start_line": profile["start"], "end_line": profile["end"], "snapshot": profile["span"], "sha256": profile["sha256"]},
            "license": {"spdx": "Apache-2.0", "source_path": "LICENSE.MD", "snapshot": LICENSE, "sha256": PINS[LICENSE][0], "git_blob_sha1": PINS[LICENSE][1]},
            "commit": {"snapshot": COMMIT, "sha256": digest(commit), "git_commit_sha1": PIN, "root_tree_sha1": TREE},
            "tree": {"manifest": TREE_MANIFEST, "sha256": TREE_SHA256, "root_tree_sha1": TREE},
            "source_adaptation": {"kind": profile["adaptation"], "reference": reference, "reference_sha256": digest(reader(reference)), "helper_span_sha256": profile["sha256"]},
        }
        for group, expected_fields in expected_metadata.items():
            for key, value in expected_fields.items():
                require(p[group][key] == value, f"False {slug} {group}/{key} binding")
        require("upstream_artifact" not in p and "module_identity_obligation" not in p, "Include helpers have no canonical compiled-module claim")
        manifests[slug] = manifest
    require(json.loads(reader(GENERATED)) == prefix_generated_cases(), "Stale/changed independently generated prefix input suite")
    require(GENERATED in manifests["prefix-list"]["source_dependencies"], "Generated prefix inputs must be recorded as a dependency")
    require(json.loads(reader(GENERATED_ATOMS)) == atom_generated_cases(), "Stale/changed independently generated atom input suite")
    require(GENERATED_ATOMS in manifests["atom-not-equal"]["source_dependencies"], "Generated atom inputs must be recorded as a dependency")
    return manifests


def expected_last(tree):
    while isinstance(tree, tuple):
        first, rest = tree
        if rest == b"":
            return {"status": "accepted", "output_hex": codec["serialize"](first).hex()}
        tree = rest
    return {"status": "rejected"}


def expected_prefix(tree, *, loose=False, zero_nil=False, reverse=False, drop_tail=False, proper_tail=False):
    if not isinstance(tree, tuple) or not isinstance(tree[1], tuple) or (not loose and tree[1][1] != b""):
        return {"status": "rejected"}
    cursor, tail = tree[0], tree[1][0]
    heads = []
    while isinstance(cursor, tuple):
        heads.append(cursor[0])
        cursor = cursor[1]
    if cursor != b"" and not (zero_nil and not any(cursor)):
        return {"status": "rejected"}
    if proper_tail:
        ending = tail
        while isinstance(ending, tuple): ending = ending[1]
        if ending != b"": return {"status": "rejected"}
    if drop_tail: tail = b""
    for head in heads if reverse else reversed(heads): tail = head, tail
    return {"status": "accepted", "output_hex": codec["serialize"](tail).hex()}


def expected_atom_not_equal(tree, *, compare=None, loose=False, allow_pairs=False):
    if not isinstance(tree, tuple) or not isinstance(tree[1], tuple) or (not loose and tree[1][1] != b""):
        return {"status": "rejected"}
    left, right = tree[0], tree[1][0]
    if isinstance(left, tuple) or isinstance(right, tuple):
        if not allow_pairs:
            return {"status": "rejected"}
        left, right = codec["serialize"](left), codec["serialize"](right)
    differs = left != right if compare is None else compare(left, right)
    return {"status": "accepted", "output_hex": "01" if differs else "80"}


def expected(slug, solution):
    require(slug in PROFILES, "Unknown helper expectation profile")
    tree = codec["parse"](solution)
    if slug == "prefix-list": return expected_prefix(tree)
    if slug == "atom-not-equal": return expected_atom_not_equal(tree)
    return expected_last(tree)


def generated_tree_fits_host_budget(tree):
    """Test-data feasibility only; this is never a public helper predicate."""
    pending = [(tree, 0)]
    nodes = atom_bytes = 0
    while pending:
        value, depth = pending.pop()
        nodes += 1
        if depth > 128 or nodes > 16_384:
            return False
        if isinstance(value, tuple):
            pending.extend([(value[0], depth + 1), (value[1], depth + 1)])
        else:
            atom_bytes += len(value)
            if atom_bytes > 1_048_576:
                return False
    return True


def validate(report, reader=read_source):
    manifests = sources(reader)
    require(report.get("vm_failure_protocol") == PROTOCOL, "Missing typed VM failure protocol")
    helpers = [p["slug"] for p in report["puzzles"] if p["provenance"]["classification"] == "canonical_helper_port"]
    expected_helpers = set(PROFILES) | {"insertion-sort-by-keys"}
    require(set(helpers) == expected_helpers and len(helpers) == len(expected_helpers), "Unknown/missing/duplicate published helper evidence")
    # This helper comes from a different repository and has its own macro and
    # relation contract. Run its independent profile rather than quietly skipping
    # a newly classified canonical adapter in this aggregate gate.
    runpy.run_path(str(ROOT / "scripts/check-insertion-sort.py"))["validate"](report, reader=reader)
    results = []
    for slug, profile in PROFILES.items():
        manifest = manifests[slug]
        puzzle = next(p for p in report["puzzles"] if p["slug"] == slug)
        require(puzzle.get("reference_mode", "module") == "module", "Helper report must use its original module adapter")
        require(puzzle.get("environment", "strict") == profile["environment"], f"Changed report ABI: {slug}")
        for field in ("title", "kind", "security_scope", "provenance"):
            require(puzzle[field] == manifest[field], f"Changed helper claim/contract: {slug}/{field}")
        require(puzzle["validated"] is True and puzzle["source"] == reader(f"puzzles/{slug}/puzzle.rs").decode()
                and puzzle["reference"] == reader(f"puzzles/{slug}/reference.clsp").decode(), "Stale/unvalidated helper sources")
        rue = puzzle["rue"]
        path = f"puzzles/{slug}/puzzle.rue"
        require(rue["path"] == path and rue["source"] == reader(path).decode() and rue["sha256"] == digest(reader(path)), "Stale Rue helper source")
        require(rue["dependencies"] == [], "These self-contained Rue helpers have no declared file imports")
        require([dependency["path"] for dependency in puzzle["source_dependencies"]] == manifest["source_dependencies"], "Missing/reordered recorded helper inputs")
        for dependency in puzzle["source_dependencies"]:
            payload = reader(dependency["path"])
            require(dependency["source"] == payload.decode() and dependency["sha256"] == digest(payload), "Stale recorded helper input")
        require(not puzzle["simulator"] and not puzzle["simulator_comparisons"] and "module_identity" not in puzzle and "factory_checks" not in puzzle,
                "Helper evidence must not claim ledger/factory/module identity")
        require([(c["label"], c["solution"]) for c in puzzle["cases"]] == [(c["label"], c["solution"]) for c in manifest["cases"]], "Missing/changed helper vectors")
        require([(a["backend"], a["optimize_lir"]) for a in puzzle["artifacts"]] == MODE_ORDER, "Missing/reordered helper compiler mode")
        for artifact in puzzle["artifacts"]:
            require(type(artifact["optimize_lir"]) is bool and type(artifact["bytes"]) is int, "Malformed helper build options/size")
            binary = bytes.fromhex(artifact["clvm_hex"])
            require(artifact["bytes"] == len(binary) and codec["tree_hash"](codec["deserialize"](binary)).hex() == artifact["tree_hash"], "Helper artifact bytes/hash conflict")
        accepted = rejected = 0
        for case in puzzle["cases"]:
            wanted = expected(slug, case["solution"])
            check_native_outcome(case["native"], case["label"])
            require(case["native"] == wanted, f"Independent {profile['helper']} expectation differs: {case['label']}")
            require([(r["backend"], r["optimize_lir"]) for r in case["runs"]] == MODE_ORDER, "Incomplete/reordered helper executions")
            for run in case["runs"]:
                require(type(run["optimize_lir"]) is bool, "Malformed helper execution mode")
                require(check_execution(run["execution"], case["label"]) == wanted, "Helper differs from the independent data relation")
            accepted += wanted["status"] == "accepted"
            rejected += wanted["status"] == "rejected"
        generated = {"last-list-element": 2103, "prefix-list": len(prefix_generated_cases()), "atom-not-equal": len(atom_generated_cases())}[slug]
        require(type(puzzle["generated_cases"]) is int and puzzle["generated_cases"] == generated, f"Changed generated helper coverage: {slug}")
        results.append((slug, len(puzzle["cases"]), accepted, rejected, generated))
    return results


def self_test(report=None):
    manifests = sources()
    mutations = [("source", {SOURCE: b"changed"}), ("license", {LICENSE: b"changed"}),
                 ("commit", {COMMIT: read_source(COMMIT) + b"\n"}), ("tree", {TREE_MANIFEST: read_source(TREE_MANIFEST) + b"\n"}),
                 ("generated", {GENERATED: b"[]"}), ("atom source", {ATOM_SOURCE: b"changed"}),
                 ("atom generated", {GENERATED_ATOMS: b"[]"}),
                 ("atom adapter separator", {"puzzles/atom-not-equal/reference.clsp": read_source("puzzles/atom-not-equal/reference.clsp").replace(b"(not (= A B)))\n", b"(not (= A B)))", 1)})]
    for slug, profile in PROFILES.items():
        path = f"puzzles/{slug}/puzzle.json"
        reference = f"puzzles/{slug}/reference.clsp"
        mutations += [(f"{slug}/span", {profile["span"]: read_source(profile["span"]) + b"\n"}),
                      (f"{slug}/wrapper", {reference: read_source(reference) + b"\n"})]
        for field in ("classification", "blob", "span", "license", "wrapper", "references", "reference_mode", "abi", "dependencies"):
            bad = copy.deepcopy(manifests[slug])
            if field == "classification": bad["provenance"]["classification"] = "canonical_puzzle_port"
            elif field == "blob": bad["provenance"]["upstream"]["git_blob_sha1"] = "0" * 40
            elif field == "span": bad["provenance"]["source_span"]["start_line"] = 99
            elif field == "license": bad["provenance"]["license"]["git_blob_sha1"] = "0" * 40
            elif field == "wrapper": bad["provenance"]["source_adaptation"]["kind"] = "none"
            elif field == "references": bad["provenance"]["references"][0] = "https://github.com/Chia-Network/chia-gaming/blob/main/clsp/std/last.clinc"
            elif field == "reference_mode": bad["reference_mode"] = "expression"
            elif field == "dependencies": bad["source_dependencies"] = []
            else: bad["environment"] = "ignored"
            mutations.append((f"{slug}/{field}", {path: json.dumps(bad).encode()}))
    for label, overrides in mutations:
        try: sources(lambda path: overrides.get(path, read_source(path)))
        except (ValueError, KeyError): continue
        raise ValueError(f"Provenance mutation passed: {label}")
    controls = len(mutations)
    for slug, solution, output in [("last-list-element", "(1 . 0x00)", None), ("last-list-element", "(())", "80"),
                                   ("last-list-element", "(1 (2 . 3))", "ff0203"), ("prefix-list", "(() 0x00)", "00"),
                                   ("prefix-list", "((1 2) (3 . 4))", "ff01ff02ff0304"),
                                   ("prefix-list", "((1 . 0x0000) ())", None), ("prefix-list", "(() () . 0x00)", None)]:
        wanted = {"status": "rejected"} if output is None else {"status": "accepted", "output_hex": output}
        require(expected(slug, solution) == wanted, "Independent relation confused nil, bytes, arbitrary P or list order")
    for name, flags in [("loose ABI", {"loose": True}), ("numeric zero as nil", {"zero_nil": True}),
                        ("reversed heads", {"reverse": True}), ("discard P", {"drop_tail": True}), ("proper P only", {"proper_tail": True})]:
        rows = manifests["prefix-list"]["cases"] + prefix_generated_cases()
        require(any(expected_prefix(codec["parse"](row["solution"]), **flags) != expected("prefix-list", row["solution"]) for row in rows), f"No witness detects relation mutation: {name}")
        controls += 1
    for row in prefix_generated_cases():
        require(generated_tree_fits_host_budget(codec["parse"](row["solution"])), "Generated input exceeds the host whole-tree comparison budget")
        outcome = expected("prefix-list", row["solution"])
        if outcome["status"] == "accepted":
            require(generated_tree_fits_host_budget(codec["deserialize"](bytes.fromhex(outcome["output_hex"]))),
                    "Generated successful output exceeds the host whole-tree comparison budget")
    for solution, output in [("(() ())", "80"), ("(() 0x00)", "01"), ("(0x01 0x0001)", "01"),
                             ("((1 . 2) (1 . 2))", None), ("(1 1 . 0x00)", None)]:
        wanted = {"status": "rejected"} if output is None else {"status": "accepted", "output_hex": output}
        require(expected("atom-not-equal", solution) == wanted, "Atom relation confused bytes, pairs or strict ABI")

    def normalized_text(value):
        try: return unicodedata.normalize("NFC", value.decode("utf8"))
        except UnicodeDecodeError: return value

    atom_mutations = [
        ("numeric normalization", {"compare": lambda left, right: int.from_bytes(left, "big", signed=True) != int.from_bytes(right, "big", signed=True)}),
        ("first32 bytes only", {"compare": lambda left, right: left[:32] != right[:32]}),
        ("ignore proper-prefix length", {"compare": lambda left, right: not (left.startswith(right) or right.startswith(left))}),
        ("Unicode normalization", {"compare": lambda left, right: normalized_text(left) != normalized_text(right)}),
        ("inverted result", {"compare": lambda left, right: left == right}),
        ("loose outer ABI", {"loose": True}), ("general tree equality", {"allow_pairs": True}),
    ]
    atom_cases = manifests["atom-not-equal"]["cases"] + atom_generated_cases()
    for name, options in atom_mutations:
        require(any(expected_atom_not_equal(codec["parse"](row["solution"]), **options) != expected("atom-not-equal", row["solution"])
                    for row in atom_cases), f"No witness detects atom relation mutation: {name}")
        controls += 1
    for row in atom_generated_cases():
        require(generated_tree_fits_host_budget(codec["parse"](row["solution"])), "Generated atom input exceeds host comparison budget")
    if report is not None:
        validate(report)
        for slug in PROFILES:
            for mutation in ("missing_case", "native", "vm", "missing_failure", "resource", "unknown", "ledger", "identity", "scope", "coverage", "reference_mode", "abi", "mode_order"):
                bad = copy.deepcopy(report)
                p = next(p for p in bad["puzzles"] if p["slug"] == slug)
                rejected = next(c for c in p["cases"] if c["native"]["status"] == "rejected")
                execution = rejected["runs"][0]["execution"]
                if mutation == "missing_case": p["cases"].pop()
                elif mutation == "native": rejected["native"] = {"status": "accepted", "output_hex": "80"}
                elif mutation == "vm": execution["outcome"] = {"status": "accepted", "output_hex": "80"}
                elif mutation == "missing_failure": del execution["failure"]
                elif mutation == "resource": execution["failure"] = {"kind": "resource", "code": "cost_exceeded"}
                elif mutation == "unknown": execution["failure"]["code"] = "unknown_code"
                elif mutation == "ledger": p["simulator"] = [{"name": "invented spend"}]
                elif mutation == "identity": p["module_identity"] = {"upstream_hex": "01"}
                elif mutation == "scope": p["provenance"]["classification"] = "canonical_puzzle_port"
                elif mutation == "reference_mode": p["reference_mode"] = "expression"
                elif mutation == "abi": p["environment"] = "ignored"
                elif mutation == "mode_order": p["artifacts"].reverse()
                else: p["generated_cases"] -= 1
                try: validate(bad)
                except (ValueError, KeyError): controls += 1; continue
                raise ValueError(f"False helper evidence passed: {slug}/{mutation}")
    return controls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=REPORT)
    parser.add_argument("--sources-only", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        report = None if args.sources_only else json.loads(args.report.read_text())
        if report is None:
            sources()
            runpy.run_path(str(ROOT / "scripts/check-insertion-sort.py"))["sources"]()
            print("PASS: four pinned helper/license/tree/span profiles and explicit raw/strict wrappers (execution not checked)")
        else:
            for slug, count, accepted, rejected, generated in validate(report):
                print(f"PASS {slug}: exact helper + explicit adapter; {count} named ({accepted} accepted/{rejected} rejected), {generated} generated, five builds; no whole-module identity claim")
        if args.self_test:
            scope = "source/provenance/relation (execution not checked)" if report is None else "provenance/executed-evidence"
            print(f"PASS: {self_test(report)} helper {scope} mutation controls")
    except (ValueError, KeyError, TypeError, OSError) as error:
        parser.exit(1, f"Canonical helper check failed: {error}\n")


if __name__ == "__main__":
    main()
