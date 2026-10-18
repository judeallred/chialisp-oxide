#!/usr/bin/env python3
"""Check a private algorithm model, independent theorem types and finite bridge.

Lean's compiler/kernel and imported standard library remain trusted components.
The model is manually authored: no result proves Rust/MIR/IR/CLVM preservation,
termination/resource equivalence of those implementations, or production safety.
"""
import argparse
import copy
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from vm_failures import PROTOCOL, check_execution, check_native_outcome, execution_sources
from cargo_authoring import check_recorded, contained_file

CODEC = runpy.run_path(str(ROOT / "scripts/check-canonical-factories.py"))
HELPERS = runpy.run_path(str(ROOT / "scripts/check-canonical-helpers.py"))
LEAN = Path(shutil.which("lean") or "lean")
LEAN_COMMIT = "eb6cafb7290e8bd5f2937522d8d804f58f02bafc"
SCOPE = "Manually authored finite-tree algorithm theorems plus finite recorded execution bridge; no Rust/MIR/IR/CLVM preservation proof."
LEAN_ARGUMENTS = ["-E", "warning", "--json", "-t0"]
LIMITATIONS = [
    "Lean compiler/kernel and standard-library trust remain; this is not independent proof-checker validation.",
    "The definitions are manual models, not extracted Rust or verified CLVM semantics.",
    "Current corpus VM inputs/outputs are finite observations; optional replay rechecks their bytes/costs/failure codes.",
    "The Rust/Rue/Chialisp bridge is finite execution evidence, not a compiler-preservation or recursive runtime-IR proof.",
]
ORACLE_SHA256 = "3ec7c455c87a0468b4445a0fe2f0aace374824219a3490fb614ef82d77f78d08"
NAMES = (
    "decode_listTree", "decode_sound", "decode_eq_some_iff", "decode_none_iff_not_proper",
    "prefix_correct", "prefix_listTree", "prefix_none_iff_decode_none", "prefix_failure_iff",
    "prefix_success_iff", "foldr_preserves_tail", "prefix_preserves_tail", "lastInner_correct",
    "lastInner_listTree", "lastInner_rejects_atom", "lastInner_rejects_nonempty_tail",
    "strictTwo_exact", "strictTwo_rejects_extra", "strictTwo_rejects_nonempty_terminator",
    "prefixEntry_exact", "prefixEntry_rejects_extra", "prefixEntry_rejects_nonempty_terminator",
)
ALLOWED_AXIOMS = {"propext", "Classical.choice", "Quot.sound"}
AUDIT = re.compile(r"'TreeTheorems\.([A-Za-z_][A-Za-z0-9_]*)' (?:does not depend on any axioms|depends on axioms: \[([^\]]*)\])")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(payload):
    return hashlib.sha256(payload).hexdigest()


def no_duplicates(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def loads(payload):
    return json.loads(payload, object_pairs_hook=no_duplicates)


def messages(stdout, stderr):
    require(not stderr.strip(), f"Unexpected Lean stderr: {stderr[:500]}")
    rows = []
    for line in stdout.splitlines():
        require(line.strip(), "Empty/non-JSON Lean protocol line")
        row = loads(line)
        require(isinstance(row, dict) and row.get("severity") == "information"
                and isinstance(row.get("data"), str), "Lean warning/error/unknown diagnostic")
        rows.append(row)
    return rows


def audit_axioms(rows):
    found = {}
    for row in rows:
        match = AUDIT.fullmatch(row["data"])
        require(match is not None, f"Unknown Lean audit message: {row['data'][:160]}")
        name, dependencies = match.groups()
        require(name in NAMES and name not in found, f"Unknown/duplicate theorem audit: {name}")
        axioms = [] if dependencies is None or not dependencies.strip() else dependencies.split(", ")
        require(len(axioms) == len(set(axioms)) and set(axioms) <= ALLOWED_AXIOMS,
                f"Unreviewed/sorry/native axiom dependency: {name}: {axioms}")
        found[name] = axioms
    require(set(found) == set(NAMES) and len(found) == 21, "Missing expected theorem axiom audit")
    return [{"name": name, "axioms": found[name]} for name in NAMES]


def run_lean(lean, directory, name, *, output=False):
    command = [str(lean), *LEAN_ARGUMENTS]
    if output:
        command += ["-o", str(directory / f"{name}.olean")]
    command.append(str(directory / f"{name}.lean"))
    environment = {**os.environ, "LEAN_PATH": str(directory)}
    result = subprocess.run(command, cwd=directory, env=environment, capture_output=True, text=True, timeout=120)
    try:
        rows = messages(result.stdout, result.stderr)
    except ValueError as error:
        raise ValueError(f"{name}: {error}") from error
    require(result.returncode == 0, f"Lean {name} failed with exit {result.returncode}")
    return rows


def check_kernel(lean, model, oracle, bridge=None):
    require(sha(oracle) == ORACLE_SHA256, "Expected-type oracle coverage/types changed; review its fixed 21 obligations")
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-lean-model-") as temporary:
        directory = Path(temporary)
        (directory / "TreeTheorems.lean").write_bytes(model)
        (directory / "ProofObligations.lean").write_bytes(oracle)
        axioms = audit_axioms(run_lean(lean, directory, "TreeTheorems", output=True))
        require(not run_lean(lean, directory, "ProofObligations"), "Unexpected expected-type oracle messages")
        if bridge is not None:
            (directory / "RecordedCases.lean").write_text(bridge)
            require(not run_lean(lean, directory, "RecordedCases"), "Unexpected finite bridge messages")
        return axioms


def artifact(row, label):
    payload = bytes.fromhex(row["clvm_hex"])
    require(row["bytes"] == len(payload), f"{label}: serialized size mismatch")
    require(CODEC["tree_hash"](CODEC["deserialize"](payload)).hex() == row["tree_hash"], f"{label}: tree hash mismatch")
    return {"label": label, "sha256": sha(payload), "bytes": len(payload), "tree_hash": row["tree_hash"]}


def finite_inputs(corpus=None):
    corpus_path = ROOT / "corpus/validation.json"
    corpus = loads(corpus_path.read_bytes()) if corpus is None else corpus
    require(corpus["schema_version"] == 1, "Unexpected corpus report schema")
    require(corpus["execution_vm"] == "clvmr 0.21.0 MEMPOOL_MODE", "Unexpected finite execution VM")
    require(corpus["vm_failure_protocol"] == PROTOCOL, "Unexpected recorded VM failure protocol")
    # This checks the exact pinned source/license/commit/tree/span identities,
    # visible raw/strict adapters, current source/dependency snapshots, independent
    # byte-preserving data relation and all five typed VM outcomes per case.
    HELPERS["validate"](corpus)
    check_recorded(corpus.get("authoring"), ROOT, [p["slug"] for p in corpus["puzzles"]])
    inputs = [corpus_path, ROOT / "scripts/check-canonical-helpers.py",
              ROOT / "scripts/check-insertion-sort.py",
              ROOT / "scripts/cargo_authoring.py", ROOT / "scripts/vm_failures.py",
              ROOT / "scripts/check-canonical-factories.py",
              ROOT / "corpus/validate_published_puzzles.py",
              ROOT / "crates/chialisp-oxide-vm/failure-protocol.json",
              HERE / "lean-toolchain", HERE / "lakefile.toml", HERE / "lake-manifest.json"]
    # Reuse the current aggregate reader/wire closure rather than preserving a
    # manually copied pre-append list. These are source freshness bindings, not
    # additional Lean theorem coverage or proof that the readers are correct.
    inputs += [contained_file(ROOT, row["path"]) for row in execution_sources()]
    inputs += [contained_file(ROOT, source["path"]) for source in corpus["authoring"]["sources"]]
    cases, replays, artifacts = [], [], []
    for slug, algorithm, count in [("prefix-list", "prefixEntry", 69), ("last-list-element", "lastInner", 16)]:
        selected = [p for p in corpus["puzzles"] if p["slug"] == slug]
        require(len(selected) == 1 and len(selected[0]["cases"]) == count, f"Missing/changed {slug} named cases")
        puzzle = selected[0]
        manifest = f"puzzles/{slug}/puzzle.json"
        inputs.append(contained_file(ROOT, manifest))
        for filename in ("puzzle.rs", "reference.clsp", "puzzle.rue"):
            inputs.append(contained_file(ROOT, f"puzzles/{slug}/{filename}"))
        inputs += [contained_file(ROOT, dependency["path"])
                   for dependency in [*puzzle["source_dependencies"], *puzzle["rue"]["dependencies"]]]
        provenance = puzzle["provenance"]
        inputs += [contained_file(ROOT, provenance[group]["snapshot"])
                   for group in ("upstream", "source_span", "license", "commit")]
        inputs.append(contained_file(ROOT, provenance["tree"]["manifest"]))
        programs = {(row["backend"], row["optimize_lir"]): row for row in puzzle["artifacts"]}
        modes = HELPERS["MODE_ORDER"]
        require(list(programs) == modes and len(puzzle["artifacts"]) == 5, f"Incomplete/reordered {slug} artifacts")
        artifacts += [artifact(row, f"{slug}/{backend}/{optimize}")
                      for (backend, optimize), row in programs.items()]
        for case in puzzle["cases"]:
            wanted = case["native"]
            check_native_outcome(wanted, f"{slug}/{case['label']}")
            # Exact atom bytes and arbitrary improper pairs survive the codec.
            tree = CODEC["parse"](case["solution"])
            output = CODEC["deserialize"](bytes.fromhex(wanted["output_hex"])) if wanted["status"] == "accepted" else None
            if output is not None:
                require(CODEC["serialize"](output).hex() == wanted["output_hex"], "Noncanonical serialized output snapshot")
            keys = [(run["backend"], run["optimize_lir"]) for run in case["runs"]]
            require(keys == modes, "Missing/reordered finite VM observation")
            for run, key in zip(case["runs"], keys):
                execution = run["execution"]
                require(check_execution(execution) == wanted, "Finite VM observation conflicts with the independent data relation")
                replays.append({"program": programs[key]["clvm_hex"], "solution": case["solution"], "execution": execution})
            cases.append({"algorithm": algorithm, "label": case["label"], "input": tree, "output": output})
    require(len(cases) == 85 and len(replays) == 425 and len(artifacts) == 10, "Finite bridge coverage changed")
    snapshots = [{"path": str(path.relative_to(ROOT)), "sha256": sha(path.read_bytes())} for path in dict.fromkeys(inputs)]
    return cases, replays, artifacts, snapshots


def bridge_source(cases):
    atoms = {}

    def term(tree):
        if isinstance(tree, tuple):
            return f"(.pair {term(tree[0])} {term(tree[1])})"
        if tree not in atoms:
            atoms[tree] = f"recordedAtom{len(atoms)}"
        return atoms[tree]

    examples = []
    for case in cases:
        source = term(case["input"])
        target = "none" if case["output"] is None else f"some {term(case['output'])}"
        examples.append(f"example : {case['algorithm']} {source} = {target} := by rfl")
    definitions = []
    for data, name in atoms.items():
        runs = [f"(List.replicate {len(list(run))} (UInt8.ofNat {byte}))" for byte, run in itertools.groupby(data)]
        value = " ++ ".join(runs) if runs else "([] : List UInt8)"
        definitions.append(f"private def {name} : Tree := .atom ({value})")
    return "import TreeTheorems\nopen TreeTheorems\n\n" + "\n".join(definitions) + "\n\n" + "\n".join(examples) + "\n"


def replay_vm(cli, replays):
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-model-replays-") as temporary:
        programs = {}
        for row in replays:
            if row["program"] not in programs:
                path = Path(temporary) / f"program-{len(programs)}.hex"
                path.write_text(row["program"] + "\n")
                programs[row["program"]] = path
            result = subprocess.run([str(cli), "run", str(programs[row["program"]]), "--solution", row["solution"], "--max-cost", "1000000"],
                                    capture_output=True, text=True, timeout=30)
            require(result.returncode in (0, 2), f"Finite VM replay invocation failed: {result.stderr[:200]}")
            execution = loads(result.stdout)
            check_execution(execution)
            require(execution == row["execution"], "Finite VM replay differs from the recorded execution")
            require(result.returncode == (0 if execution["outcome"]["status"] == "accepted" else 2), "VM exit/outcome mismatch")
    return len(replays)


def finite_report_controls():
    """Corrupt current evidence, without changing the theorem/type oracle."""
    original = loads((ROOT / "corpus/validation.json").read_bytes())
    controls = 0
    mutations = ("missing_prefix", "missing_last", "artifact_order", "missing_run",
                 "prefix_native", "last_native", "resource_rejection", "inconclusive",
                 "unknown_failure", "missing_failure", "provenance", "dependency", "authoring")
    for mutation in mutations:
        bad = copy.deepcopy(original)
        prefix = next(p for p in bad["puzzles"] if p["slug"] == "prefix-list")
        last = next(p for p in bad["puzzles"] if p["slug"] == "last-list-element")
        rejected = next(case for case in prefix["cases"] if case["native"]["status"] == "rejected")["runs"][0]["execution"]
        if mutation == "missing_prefix": bad["puzzles"].remove(prefix)
        elif mutation == "missing_last": bad["puzzles"].remove(last)
        elif mutation == "artifact_order": prefix["artifacts"].reverse()
        elif mutation == "missing_run": prefix["cases"][0]["runs"].pop()
        elif mutation in ("prefix_native", "last_native"):
            puzzle = prefix if mutation == "prefix_native" else last
            next(case for case in puzzle["cases"] if case["native"]["status"] == "accepted")["native"]["output_hex"] = "ff0180"
        elif mutation == "resource_rejection": rejected["failure"] = {"kind": "resource", "code": "cost_exceeded"}
        elif mutation == "inconclusive":
            rejected["outcome"] = {"status": "inconclusive"}
            rejected["failure"] = {"kind": "resource", "code": "cost_exceeded"}
        elif mutation == "unknown_failure": rejected["failure"]["code"] = "unknown_code"
        elif mutation == "missing_failure": rejected.pop("failure")
        elif mutation == "provenance": prefix["provenance"]["source_span"]["start_line"] = 8
        elif mutation == "dependency": prefix["source_dependencies"][0]["source"] += "\n// stale snapshot\n"
        elif mutation == "authoring": bad["authoring"]["exports"]["prefix-list"]["entry"] = "chialisp_oxide_puzzles::last_list_element::puzzle"
        try: finite_inputs(bad)
        except (ValueError, KeyError, TypeError): controls += 1; continue
        raise ValueError(f"Corrupted finite bridge evidence passed: {mutation}")
    return controls


def self_test(lean, model, oracle, axioms):
    rows = [{"severity": "information", "data": f"'TreeTheorems.{row['name']}' " +
             ("depends on axioms: [" + ", ".join(row["axioms"]) + "]" if row["axioms"] else "does not depend on any axioms")}
            for row in axioms]
    controls = 0
    for mutation in ("missing", "duplicate", "unknown_name", "warning", "error", "unknown_severity",
                     "unknown_message", "sorry_axiom", "native_axiom", "custom_axiom", "duplicate_axiom",
                     "malformed_axiom", "malformed_data", "non_json", "duplicate_json_key", "stderr", "blank_line"):
        bad = copy.deepcopy(rows)
        stderr = ""
        if mutation == "missing": bad.pop()
        elif mutation == "duplicate": bad.append(copy.deepcopy(bad[0]))
        elif mutation == "unknown_name": bad[0]["data"] = "'TreeTheorems.unclaimed' does not depend on any axioms"
        elif mutation in ("warning", "error", "unknown_severity"): bad[0]["severity"] = {"unknown_severity": "note"}.get(mutation, mutation)
        elif mutation == "unknown_message": bad[0]["data"] = "arbitrary informational output"
        elif mutation in ("sorry_axiom", "native_axiom", "custom_axiom", "duplicate_axiom", "malformed_axiom"):
            dependencies = {"sorry_axiom": "sorryAx", "native_axiom": "Lean.ofReduceBool", "custom_axiom": "TreeTheorems.assumption",
                            "duplicate_axiom": "propext, propext", "malformed_axiom": "propext,Classical.choice"}[mutation]
            bad[0]["data"] = f"'TreeTheorems.{NAMES[0]}' depends on axioms: [{dependencies}]"
        elif mutation == "malformed_data": bad[0]["data"] = 21
        if mutation == "stderr": stderr = "unexpected native output"
        stdout = "\n".join(json.dumps(row) for row in bad)
        if mutation == "non_json": stdout += "\nnot JSON"
        elif mutation == "duplicate_json_key": stdout += '\n{"severity":"information","severity":"warning","data":"duplicate key"}'
        elif mutation == "blank_line": stdout += "\n\n"
        try: audit_axioms(messages(stdout, stderr))
        except ValueError: controls += 1; continue
        raise ValueError(f"Axiom protocol mutation passed: {mutation}")

    # These transformations alter direct algorithms only. The specifications,
    # intended theorem statements and independent type oracle are retained.
    mutations = [
        ("drop_arbitrary_tail", "| .atom [], tail => some tail", "| .atom [], _ => some (.atom [])"),
        ("accept_improper_left", "| .atom (_ :: _), _ => none", "| .atom (_ :: _), tail => some tail"),
        ("swap_output_children", "(prefixTree rest tail).map (.pair first)", "(prefixTree rest tail).map (fun result => .pair result first)"),
        ("last_returns_first", "| .pair _ _ => lastInner rest", "| .pair _ _ => some first"),
        ("accept_nonempty_ABI_terminator", "| .pair first (.pair second (.atom [])) => some (first, second)", "| .pair first (.pair second (.atom _)) => some (first, second)"),
    ]
    text = model.decode()
    for name, before, after in mutations:
        require(text.count(before) == 1, f"Model mutation no longer has an exact source anchor: {name}")
        try: check_kernel(lean, text.replace(before, after, 1).encode(), oracle)
        except ValueError: controls += 1; continue
        raise ValueError(f"Algorithm mutation passed the retained theorems: {name}")

    original = """theorem prefixEntry_rejects_nonempty_terminator
    (first second : Tree) (byte : UInt8) (bytes : List UInt8) :
    prefixEntry (.pair first (.pair second (.atom (byte :: bytes)))) = none := by
  rfl"""
    weakened = """set_option linter.unusedVariables false in
theorem prefixEntry_rejects_nonempty_terminator
    (first second : Tree) (byte : UInt8) (bytes : List UInt8) : True := by
  trivial"""
    require(text.count(original) == 1, "Expected-type weakening control lost its exact theorem anchor")
    try: check_kernel(lean, text.replace(original, weakened, 1).encode(), oracle)
    except ValueError as error:
        require(str(error).startswith("ProofObligations:"), "Weakening control did not reach the independent expected-type oracle")
        controls += 1
    else: raise ValueError("True-weakened public theorem passed the independent expected-type oracle")

    for corrupted in [oracle.replace(b"  @decode_listTree", b"  @decode_sound", 1), oracle.replace(b"example :", b"-- missing obligation\n-- example :", 1)]:
        try: check_kernel(lean, model, corrupted)
        except ValueError: controls += 1; continue
        raise ValueError("Changed/missing expected-type coverage passed the fixed oracle pin")
    # Comment text is not a proof rule. Do not pretend a keyword ban validates
    # Lean; dependency audits and actual expected types are what matter.
    check_kernel(lean, model + b"\n/- Comment only: sorry, admit, native_decide, axiom. -/\n", oracle)
    return controls + finite_report_controls()


def expected_inputs(model, oracle, snapshots):
    require(sha(oracle) == ORACLE_SHA256, "Expected-type oracle coverage/types changed")
    return [{"path": "TreeTheorems.lean", "sha256": sha(model)},
            {"path": "ProofObligations.lean", "sha256": sha(oracle)},
            {"path": "check-model.py", "sha256": sha(Path(__file__).read_bytes())}, *snapshots]


def check_lean_version(version):
    require(isinstance(version, str) and re.fullmatch(
        rf"Lean \(version 4\.35\.0, [^,]+, commit {LEAN_COMMIT}, Release\)", version),
        "Unexpected Lean version/commit/build")


def check_recorded_result(record, context=None):
    """Freshness and shape only: this function never invokes Lean or a VM."""
    require(isinstance(record, dict) and set(record) == {
        "schema_version", "scope", "vm_failure_protocol", "lean", "inputs", "theorems",
        "expected_theorem_types", "finite_bridge", "mutation_controls", "limitations"},
        "Unexpected/missing recorded algorithm evidence fields")
    require(type(record["schema_version"]) is int and record["schema_version"] == 1, "Changed algorithm evidence schema")
    require(record["scope"] == SCOPE and record["limitations"] == LIMITATIONS, "Changed algorithm evidence claim scope")
    require(record["vm_failure_protocol"] == PROTOCOL, "Changed algorithm evidence failure protocol")
    require(type(record["expected_theorem_types"]) is int and record["expected_theorem_types"] == 21, "Changed theorem type coverage")
    require(type(record["mutation_controls"]) is int and record["mutation_controls"] == 38, "Missing full algorithm/evidence controls")
    tool = record["lean"]
    require(isinstance(tool, dict) and set(tool) == {"version", "sha256", "arguments"}
            and tool["arguments"] == LEAN_ARGUMENTS, "Changed Lean checking arguments")
    check_lean_version(tool["version"])
    require(isinstance(tool["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", tool["sha256"]), "Missing recorded Lean binary identity")
    theorems = record["theorems"]
    require(isinstance(theorems, list), "Missing recorded theorem audits")
    rows = []
    for theorem in theorems:
        require(isinstance(theorem, dict) and set(theorem) == {"name", "axioms"}
                and isinstance(theorem["name"], str) and isinstance(theorem["axioms"], list)
                and all(isinstance(value, str) for value in theorem["axioms"]), "Malformed recorded theorem audit")
        dependencies = theorem["axioms"]
        rows.append({"severity": "information", "data": f"'TreeTheorems.{theorem['name']}' " +
                     ("depends on axioms: [" + ", ".join(dependencies) + "]" if dependencies else "does not depend on any axioms")})
    require(theorems == audit_axioms(rows), "Changed/missing/reordered theorem audit names")
    cases, replays, artifacts, snapshots = finite_inputs() if context is None else context
    bridge = bridge_source(cases).encode()
    require(record["inputs"] == expected_inputs((HERE / "TreeTheorems.lean").read_bytes(),
            (HERE / "ProofObligations.lean").read_bytes(), snapshots), "Stale algorithm source, corpus, adapter or provenance input hashes")
    finite = record["finite_bridge"]
    require(isinstance(finite, dict) and set(finite) == {
        "cases", "prefix_cases", "last_inner_cases", "recorded_vm_observations", "generated_lean_sha256",
        "generated_lean_bytes", "vm_replays_executed", "artifacts", "replay_cli_sha256"}, "Missing full finite execution bridge")
    for name, value in {"cases": 85, "prefix_cases": 69, "last_inner_cases": 16,
                        "recorded_vm_observations": 425, "vm_replays_executed": 425,
                        "generated_lean_bytes": len(bridge)}.items():
        require(type(finite[name]) is int and finite[name] == value, f"Changed finite bridge coverage: {name}")
    require(len(replays) == 425 and finite["generated_lean_sha256"] == sha(bridge)
            and finite["artifacts"] == artifacts and len(artifacts) == 10,
            "Stale finite bridge or compiled artifact identities")
    require(isinstance(finite["replay_cli_sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", finite["replay_cli_sha256"]),
            "Missing recorded VM replay binary identity")
    return context if context is not None else (cases, replays, artifacts, snapshots)


def recorded_result_controls(record, context):
    mutations = ("scope", "limitation", "missing_theorem", "duplicate_theorem", "unknown_theorem", "sorry_axiom",
                 "tool_version", "tool_arguments", "input_hash", "missing_input", "bridge_hash", "case_count",
                 "missing_replay", "artifact_hash", "controls", "missing_binary")
    controls = 0
    for mutation in mutations:
        bad = copy.deepcopy(record)
        if mutation == "scope": bad["scope"] = "Proves compiler preservation"
        elif mutation == "limitation": bad["limitations"].pop()
        elif mutation == "missing_theorem": bad["theorems"].pop()
        elif mutation == "duplicate_theorem": bad["theorems"].append(copy.deepcopy(bad["theorems"][0]))
        elif mutation == "unknown_theorem": bad["theorems"][0]["name"] = "unreviewed"
        elif mutation == "sorry_axiom": bad["theorems"][0]["axioms"] = ["sorryAx"]
        elif mutation == "tool_version": bad["lean"]["version"] = "Lean 4.35.0"
        elif mutation == "tool_arguments": bad["lean"]["arguments"].remove("-t0")
        elif mutation == "input_hash": bad["inputs"][0]["sha256"] = "0" * 64
        elif mutation == "missing_input": bad["inputs"].pop()
        elif mutation == "bridge_hash": bad["finite_bridge"]["generated_lean_sha256"] = "0" * 64
        elif mutation == "case_count": bad["finite_bridge"]["cases"] -= 1
        elif mutation == "missing_replay": bad["finite_bridge"]["vm_replays_executed"] -= 1
        elif mutation == "artifact_hash": bad["finite_bridge"]["artifacts"][0]["sha256"] = "0" * 64
        elif mutation == "controls": bad["mutation_controls"] = 25
        elif mutation == "missing_binary": bad["finite_bridge"].pop("replay_cli_sha256")
        try: check_recorded_result(bad, context)
        except (ValueError, KeyError, TypeError): controls += 1; continue
        raise ValueError(f"Corrupted recorded algorithm evidence passed: {mutation}")
    return controls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lean", type=Path, default=LEAN)
    parser.add_argument("--model", type=Path, default=HERE / "TreeTheorems.lean")
    parser.add_argument("--oracle", type=Path, default=HERE / "ProofObligations.lean")
    parser.add_argument("--replay-vm", action="store_true")
    parser.add_argument("--check-recorded", type=Path, nargs="?", const=HERE / "results.json",
                        help="check recorded evidence freshness without executing Lean or the VM")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/chialisp-oxide")
    parser.add_argument("--output", type=Path, help="optional reproducible evidence record")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.check_recorded is not None:
            require(not args.replay_vm and args.output is None, "Freshness-only checking cannot replay or generate evidence")
            record = loads(args.check_recorded.read_bytes())
            context = check_recorded_result(record)
            print("PASS: recorded algorithm evidence is current; 21 theorem names/types, 85 cases, 425 recorded replays, 10 artifacts, 38 full-run controls")
            if args.self_test:
                print(f"PASS: {recorded_result_controls(record, context)} recorded-freshness corruption controls")
            print("Freshness only: Lean and the VM were not executed; recorded tool binary hashes were not independently revalidated")
            return
        version = subprocess.run([str(args.lean), "--version"], check=True, capture_output=True, text=True, timeout=15).stdout.strip()
        check_lean_version(version)
        model, oracle = args.model.read_bytes(), args.oracle.read_bytes()
        cases, replays, artifacts, inputs = finite_inputs()
        bridge = bridge_source(cases)
        axioms = check_kernel(args.lean.resolve(), model, oracle, bridge)
        replayed = replay_vm(args.cli.resolve(), replays) if args.replay_vm else 0
        controls = self_test(args.lean.resolve(), model, oracle, axioms) if args.self_test else None
        result = {
            "schema_version": 1, "scope": SCOPE,
            "vm_failure_protocol": PROTOCOL,
            "lean": {"version": version, "sha256": sha(args.lean.read_bytes()), "arguments": LEAN_ARGUMENTS},
            "inputs": expected_inputs(model, oracle, inputs),
            "theorems": axioms, "expected_theorem_types": 21,
            "finite_bridge": {"cases": 85, "prefix_cases": 69, "last_inner_cases": 16, "recorded_vm_observations": 425,
                              "generated_lean_sha256": sha(bridge.encode()), "generated_lean_bytes": len(bridge.encode()),
                              "vm_replays_executed": replayed, "artifacts": artifacts},
            "mutation_controls": controls,
            "limitations": LIMITATIONS,
        }
        if args.replay_vm:
            result["finite_bridge"]["replay_cli_sha256"] = sha(args.cli.read_bytes())
        if args.output:
            args.output.write_text(json.dumps(result, indent=2) + "\n")
        print(f"PASS: 21 fixed theorem types and exact axiom audits; 85 kernel-checked finite bridge equalities; {replayed} VM replays")
        if controls is not None:
            print(f"PASS: {controls} protocol/type/model mutation controls")
        print("Scope: algorithm model + finite bridge, not Rust/MIR/CLVM preservation or production readiness")
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Tree model check failed: {error}\n")


if __name__ == "__main__":
    main()
