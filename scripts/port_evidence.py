"""Manifest-driven checks shared by pinned port evidence gates.

A port's `puzzles/<slug>/puzzle.json` declares its provenance pins, sources,
dependencies, named vectors and declared divergences. This module checks the
recorded corpus report against that data and against an independent expected
outcome per input: either an oracle supplied by the caller or outcomes declared
in the manifest. It reads reports and sources; it does not execute a compiler,
and `replay_port` only re-executes retained CLVM bytes through the pinned VM.
None of these checks is a compiler-preservation proof.
"""
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

from cargo_authoring import check_recorded, contained_file, unique_object
from ledger_rejections import check_ledger_report
from vm_failures import PROTOCOL, check_execution, check_native_outcome
import raw_tree

ROOT = Path(__file__).resolve().parents[1]
MODES = [("rust_mir", False), ("rust_mir", True), ("chialisp", True), ("rue_source", False), ("rue_source", True)]
DEFAULT_BUDGET = 1_000_000
MAX_BUDGET = 10_000_000
DIVERGENCE_BACKENDS = {"chialisp", "rue_source"}
PATH_KEYS = ("snapshot", "path", "manifest")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_source(path):
    return contained_file(ROOT, path).read_bytes()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def git_object(kind, value):
    return hashlib.sha1(f"{kind} {len(value)}\0".encode() + value).hexdigest()


def load_json(payload):
    return json.loads(payload, object_pairs_hook=unique_object)


def load_manifest(slug, reader=read_source):
    require(re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", slug or ""), "Malformed puzzle slug")
    manifest = load_json(reader(f"puzzles/{slug}/puzzle.json"))
    require(manifest["slug"] == slug and manifest["cases"] and isinstance(manifest["provenance"], dict), f"{slug}: malformed manifest")
    return manifest


def puzzle_row(report, slug):
    rows = [row for row in report["puzzles"] if row.get("slug") == slug]
    require(len(rows) == 1, f"{slug}: missing or duplicate corpus row")
    return rows[0]


def check_header(report):
    require(type(report.get("schema_version")) is int and report["schema_version"] == 1
            and report.get("vm_failure_protocol") == PROTOCOL, "Missing current typed VM-failure protocol/schema")
    slugs = [row.get("slug") for row in report["puzzles"]]
    require(len(slugs) == len(set(slugs)), "Duplicate corpus puzzle")
    check_recorded(report["authoring"], ROOT, slugs)


def check_pins(provenance, reader=read_source, label="provenance"):
    """Every pinned snapshot/path with a sha256 or Git object ID must match the retained bytes."""
    checked = 0

    def walk(value, trail):
        nonlocal checked
        if isinstance(value, dict):
            paths = [value[key] for key in PATH_KEYS if isinstance(value.get(key), str)]
            if paths:
                data = reader(paths[0])
                for key in ("sha256", "source_sha256"):
                    if key in value:
                        require(value[key] == digest(data), f"{label}/{trail}: pinned SHA-256 changed for {paths[0]}")
                        checked += 1
                if "git_blob_sha1" in value:
                    require(value["git_blob_sha1"] == git_object("blob", data), f"{label}/{trail}: pinned Git blob changed for {paths[0]}")
                    checked += 1
                if "git_commit_sha1" in value:
                    require(value["git_commit_sha1"] == git_object("commit", data), f"{label}/{trail}: pinned Git commit changed for {paths[0]}")
                    checked += 1
                if "clvm_hex" in value:
                    text = data.decode().strip()
                    program = bytes.fromhex(value["clvm_hex"])
                    require(text == value["clvm_hex"] and value.get("bytes") == len(program)
                            and raw_tree.tree_hash(raw_tree.deserialize(program)).hex() == value.get("tree_hash"),
                            f"{label}/{trail}: published artifact bytes/size/hash changed for {paths[0]}")
                    checked += 1
            for key, item in value.items():
                walk(item, f"{trail}.{key}")
        elif isinstance(value, list):
            for index, item in enumerate(value):
                walk(item, f"{trail}[{index}]")

    walk(provenance, "provenance")
    return checked


def check_identity(puzzle, manifest, reader=read_source):
    slug = manifest["slug"]
    for field in ("slug", "title", "kind", "security_scope", "provenance"):
        require(puzzle.get(field) == manifest[field], f"{slug}: stale or changed port claim: {field}")
    require(puzzle.get("environment", "strict") == manifest.get("environment", "strict")
            and puzzle.get("reference_mode", "module") == manifest.get("reference_mode", "module")
            and puzzle.get("validated") is True, f"{slug}: changed or unvalidated environment/reference contract")
    base = f"puzzles/{slug}"
    require(puzzle["source"] == reader(base + "/puzzle.rs").decode()
            and puzzle["reference"] == reader(base + "/reference.clsp").decode(), f"{slug}: stale Rust/Chialisp source")
    rue = puzzle["rue"]
    payload = reader(base + "/puzzle.rue")
    dependencies = []
    for path in manifest.get("rue_dependencies", []):
        data = reader(path)
        dependencies.append({"path": path, "sha256": digest(data), "source": data.decode()})
    require(rue == {"path": base + "/puzzle.rue", "source": payload.decode(), "sha256": digest(payload), "dependencies": dependencies},
            f"{slug}: stale or unknown Rue source/dependencies")


def artifact_bytes(artifact, label):
    require(isinstance(artifact, dict) and type(artifact.get("optimize_lir")) is bool
            and type(artifact.get("bytes")) is int and artifact["bytes"] > 0, f"{label}: malformed artifact mode/size")
    text = artifact.get("clvm_hex")
    require(isinstance(text, str) and re.fullmatch(r"(?:[0-9a-f]{2})+", text), f"{label}: malformed serialized CLVM hex")
    binary = bytes.fromhex(text)
    require(len(binary) == artifact["bytes"] and len(binary) <= 2_000_000, f"{label}: artifact size mismatch/bound")
    tree = raw_tree.deserialize(binary)
    require(raw_tree.serialize(tree) == binary, f"{label}: artifact is not canonically serialized")
    require(raw_tree.tree_hash(tree).hex() == artifact.get("tree_hash"), f"{label}: artifact tree hash mismatch")
    require(isinstance(artifact.get("disassembly"), str) and artifact["disassembly"], f"{label}: missing artifact disassembly")
    return binary


def check_modes(rows, label):
    require(isinstance(rows, list) and [(row.get("backend"), row.get("optimize_lir")) for row in rows] == MODES
            and all(type(row.get("optimize_lir")) is bool for row in rows), f"{label}: missing, extra or reordered compiler modes")


def check_artifacts(puzzle):
    check_modes(puzzle["artifacts"], puzzle["slug"] + " artifacts")
    return [artifact_bytes(artifact, f"{puzzle['slug']} {artifact['backend']}/{artifact['optimize_lir']}") for artifact in puzzle["artifacts"]]


def check_module_identity(puzzle, manifest):
    slug = manifest["slug"]
    upstream = manifest["provenance"].get("upstream_artifact")
    if upstream is None:
        require("module_identity" not in puzzle, f"{slug}: module identity claimed without a published artifact pin")
        return None
    artifacts = puzzle["artifacts"]
    expected = {"upstream_hex": upstream["clvm_hex"], "upstream_tree_hash": upstream["tree_hash"],
                "comparisons": [{"backend": a["backend"], "optimize_lir": a["optimize_lir"],
                                 "serialized_bytes_equal": a["clvm_hex"] == upstream["clvm_hex"],
                                 "tree_hash_equal": a["tree_hash"] == upstream["tree_hash"]} for a in artifacts]}
    recorded = puzzle.get("module_identity")
    require(isinstance(recorded, dict) and all(type(row.get(field)) is bool for row in recorded.get("comparisons", [])
            for field in ("optimize_lir", "serialized_bytes_equal", "tree_hash_equal")), f"{slug}: identity flags must be exact booleans")
    require(recorded == expected, f"{slug}: false or missing canonical module identity comparison")
    original = artifacts[2]
    require(original["clvm_hex"] == upstream["clvm_hex"] and original["tree_hash"] == upstream["tree_hash"]
            and original["bytes"] == upstream["bytes"], f"{slug}: independent Chialisp must reproduce the published module")
    return expected


def check_dependencies(puzzle, manifest, authoring, reader=read_source):
    slug = manifest["slug"]
    declared = puzzle["source_dependencies"]
    require([row["path"] for row in declared] == manifest.get("source_dependencies", []), f"{slug}: missing, extra or reordered source dependencies")
    for row in declared:
        payload = reader(row["path"])
        require(set(row) == {"path", "sha256", "source"} and row["sha256"] == digest(payload)
                and row["source"] == payload.decode(), f"{slug}: stale dependency snapshot {row['path']}")
    require(puzzle.get("reference_includes", {}) == manifest.get("reference_includes", {}), f"{slug}: changed, missing or unknown reference include mapping")
    for path in puzzle.get("reference_includes", {}).values():
        reader(path)
    native = puzzle.get("native_sources", [])
    require(isinstance(native, list) and [row["path"] for row in native] == manifest.get("native_sources", []), f"{slug}: missing/unknown native oracle source closure")
    inputs = {row["path"]: row["sha256"] for row in authoring["sources"]}
    for path in [f"puzzles/{slug}/puzzle.rs", *manifest.get("source_dependencies", []), *manifest.get("native_sources", [])]:
        require(inputs.get(path) == digest(reader(path)), f"{slug}: port input absent or stale in the compiled native Cargo closure: {path}")
    for row in native:
        require(set(row) == {"path", "sha256"} and row["sha256"] == digest(reader(row["path"]))
                and inputs.get(row["path"]) == row["sha256"], f"{slug}: stale native source/build-time input binding")


def check_budget(puzzle, manifest):
    slug = manifest["slug"]
    declared = manifest.get("validation_max_cost")
    if declared is None:
        require("validation_max_cost" not in puzzle, f"{slug}: report must retain its omitted default comparison budget")
        return DEFAULT_BUDGET
    require(type(declared) is int and 1 <= declared <= MAX_BUDGET and puzzle.get("validation_max_cost") == declared
            and type(puzzle.get("validation_max_cost")) is int, f"{slug}: missing or changed recorded comparison budget")
    return declared


def check_cost(execution, budget, label):
    if execution["outcome"]["status"] == "accepted":
        require(type(execution["cost"]) is int and execution["cost"] <= budget, f"{label}: recorded successful execution exceeds its declared cost budget")


def declared_outcome(case):
    divergence = case.get("declared_divergence")
    if divergence is None:
        return None
    require(set(divergence) == {"backends", "outcome", "reason"} and divergence["backends"]
            and set(divergence["backends"]) <= DIVERGENCE_BACKENDS and divergence["reason"], "declared divergence may only bind the independently compiled languages")
    return divergence


def bound_outcome(case, backend, native):
    divergence = case.get("declared_divergence")
    if divergence is not None and backend in divergence["backends"]:
        return divergence["outcome"]
    return native


def declared_expectations(manifest):
    """Expected outcomes declared as reviewed data in the manifest, if every case carries one."""
    cases = manifest["cases"]
    if not all("expected" in case for case in cases):
        require(not any("expected" in case for case in cases), f"{manifest['slug']}: either every case or no case declares an expected outcome")
        return None
    table = {}
    for case in cases:
        outcome = case["expected"]
        require(outcome == {"status": "rejected"} or (set(outcome) == {"status", "output_hex"} and outcome["status"] == "accepted"
                and isinstance(outcome["output_hex"], str) and re.fullmatch(r"(?:[0-9a-f]{2})*", outcome["output_hex"])),
                f"{manifest['slug']}: malformed declared expectation for {case['label']!r}")
        require(case["solution"] not in table or table[case["solution"]] == outcome, f"{manifest['slug']}: conflicting declared expectations")
        table[case["solution"]] = outcome
    return lambda solution: table[solution]


def check_cases(puzzle, manifest, expected, budget):
    slug = manifest["slug"]
    recorded = [{key: case[key] for key in ("label", "solution", "declared_divergence") if key in case} for case in puzzle["cases"]]
    declared = [{key: case[key] for key in ("label", "solution", "declared_divergence") if key in case} for case in manifest["cases"]]
    require(recorded == declared, f"{slug}: missing, extra, reordered or relabeled named inputs")
    counts = {"named": len(puzzle["cases"]), "accepted": 0, "rejected": 0, "divergent": 0}
    for case in puzzle["cases"]:
        label = f"{slug}/{case['label']}"
        native = check_native_outcome(case["native"], label)
        if expected is not None:
            require(native == expected(case["solution"]), f"{label}: native output differs from the independent expectation")
        divergence = declared_outcome(case)
        if divergence is not None:
            require(divergence["outcome"] != native, f"{label}: declared divergence must differ from the native expectation")
            counts["divergent"] += 1
        check_modes(case["runs"], label)
        for run in case["runs"]:
            execution = check_execution(run["execution"], label)
            require(execution == bound_outcome(case, run["backend"], native), f"{label}: {run['backend']} result differs from its bound expectation")
            check_cost(run["execution"], budget, label)
        counts["accepted" if native["status"] == "accepted" else "rejected"] += 1
    return counts


def check_ledger(puzzle, report):
    slug = puzzle["slug"]
    if puzzle["kind"] == "coin_puzzle":
        require(bool(puzzle["simulator"]), f"{slug}: coin puzzle requires actual ledger scenarios")
        check_ledger_report({"simulator": report["simulator"], "puzzles": [puzzle]})
        return len(puzzle["simulator"])
    require(puzzle["simulator"] == [] and puzzle["simulator_comparisons"] == [], f"{slug}: non-spend export cannot claim ledger evidence")
    return 0


def validate_port(report, slug, *, expected=None, reader=read_source, generated=0, require_pins=True):
    """Check one port's recorded evidence against its manifest and an independent expectation."""
    manifest = load_manifest(slug, reader)
    puzzle = puzzle_row(report, slug)
    if expected is None:
        expected = declared_expectations(manifest)
    pins = check_pins(manifest["provenance"], reader, slug) if require_pins else 0
    check_identity(puzzle, manifest, reader)
    check_artifacts(puzzle)
    identity = check_module_identity(puzzle, manifest)
    check_dependencies(puzzle, manifest, report["authoring"], reader)
    budget = check_budget(puzzle, manifest)
    counts = check_cases(puzzle, manifest, expected, budget)
    require(type(puzzle["generated_cases"]) is int and puzzle["generated_cases"] == generated, f"{slug}: changed generated execution coverage count")
    ledger = check_ledger(puzzle, report)
    return {"slug": slug, **counts, "generated": generated, "builds": len(MODES), "validation_max_cost": budget,
            "pins": pins, "module_identity": identity is not None, "ledger_groups": ledger,
            "expectation": "oracle" if expected is not None else "recorded_native"}


def run_batch(cli, path, solutions, count, *, max_cost, require_conclusive=True):
    require(type(count) is int and 0 < count <= 4096, "Invalid retained replay batch size")
    require(type(max_cost) is int and 1 <= max_cost <= MAX_BUDGET, "Invalid retained replay cost budget")
    result = subprocess.run([str(cli), "run-batch", str(path), "--solutions", str(solutions), "--max-cost", str(max_cost)],
                            capture_output=True, text=True, timeout=120)
    require(result.returncode == 0, "Retained VM batch invocation failed: " + result.stderr[:2000])
    executions = load_json(result.stdout)
    require(isinstance(executions, list) and len(executions) == count, "Missing/extra retained VM batch observations")
    for execution in executions:
        check_execution(execution, "retained artifact replay", require_conclusive=require_conclusive)
        check_cost(execution, max_cost, "retained artifact replay")
    return executions


def replay_port(report, slug, cli, *, expected=None, extra_cases=(), reader=read_source):
    """Re-execute retained artifacts on named and extra inputs; no compilation."""
    manifest = load_manifest(slug, reader)
    puzzle = puzzle_row(report, slug)
    if expected is None:
        expected = declared_expectations(manifest)
    budget = check_budget(puzzle, manifest)
    cases = puzzle["cases"] + list(extra_cases)
    total = 0
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-port-replay-") as temporary:
        solutions = Path(temporary) / f"{slug}-solutions.json"
        payload = json.dumps([case["solution"] for case in cases]).encode()
        require(0 < len(cases) <= 4096 and len(payload) <= 8 * 1024 * 1024, f"{slug}: replay input batch exceeds CLI bound")
        solutions.write_bytes(payload)
        for mode_index, artifact in enumerate(puzzle["artifacts"]):
            path = Path(temporary) / f"{slug}-{mode_index}.hex"
            path.write_text(artifact_bytes(artifact, slug).hex() + "\n")
            executions = run_batch(cli, path, solutions, len(cases), max_cost=budget)
            for index, (case, execution) in enumerate(zip(cases, executions)):
                native = expected(case["solution"]) if expected is not None else case["native"]
                require(execution["outcome"] == bound_outcome(case, artifact["backend"], native),
                        f"{slug}/{case['label']}: retained artifact differs from the independent expectation")
                if index < len(puzzle["cases"]):
                    require(execution == case["runs"][mode_index]["execution"], f"{slug}/{case['label']}: named retained artifact cost/failure/output differs")
                total += 1
            print(f"PASS: {slug} {artifact['backend']} optimize={artifact['optimize_lir']}: {len(executions)} retained-byte executions", flush=True)
    return total


def json_wire(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


def self_test_port(report, slug, validate, *, reader=read_source):
    """Report, manifest and source corruption controls; every one must be rejected."""
    manifest = load_manifest(slug, reader)
    puzzle = puzzle_row(report, slug)
    original_wire = json_wire(puzzle)
    accepted_case = next((index for index, case in enumerate(puzzle["cases"]) if case["native"]["status"] == "accepted"), None)
    rejected_case = next((index for index, case in enumerate(puzzle["cases"]) if case["native"]["status"] == "rejected"), None)
    report_mutations = [
        ("renamed vector", lambda p: p["cases"][0].update(label=p["cases"][0]["label"] + " edited")),
        ("missing vector", lambda p: p["cases"].pop()),
        ("reordered vectors", lambda p: p["cases"].reverse()),
        ("missing build", lambda p: p["artifacts"].pop()),
        ("reordered builds", lambda p: p["artifacts"].reverse()),
        ("numeric build mode", lambda p: p["artifacts"][0].update(optimize_lir=0)),
        ("artifact size", lambda p: p["artifacts"][0].update(bytes=p["artifacts"][0]["bytes"] + 1)),
        ("artifact hash", lambda p: p["artifacts"][0].update(tree_hash="0" * 64)),
        ("missing run", lambda p: p["cases"][0]["runs"].pop()),
        ("stale Rust source", lambda p: p.update(source=p["source"] + "\n")),
        ("stale Chialisp source", lambda p: p.update(reference=p["reference"] + "\n")),
        ("stale Rue source", lambda p: p["rue"].update(source=p["rue"]["source"] + "\n")),
        ("changed claim scope", lambda p: p.update(security_scope="Production ready")),
        ("changed provenance", lambda p: p["provenance"].update(invented="claim")),
        ("unvalidated", lambda p: p.update(validated=1)),
        ("generated count", lambda p: p.update(generated_cases=p["generated_cases"] + 1)),
        ("invented ledger claim", lambda p: p["simulator"].append({"name": "invented", "passed": True})),
        ("invented module identity", lambda p: p.update(module_identity={"upstream_hex": "80", "upstream_tree_hash": "0" * 64, "comparisons": []})),
    ]
    if accepted_case is not None:
        report_mutations += [
            ("flipped native outcome", lambda p: p["cases"][accepted_case].update(native={"status": "rejected"})),
            ("flipped VM outcome", lambda p: p["cases"][accepted_case]["runs"][1]["execution"].update(outcome={"status": "rejected"}, failure={"kind": "program", "code": "raise"})),
            ("changed VM output", lambda p: p["cases"][accepted_case]["runs"][2]["execution"]["outcome"].update(output_hex="80")),
            ("cost over budget", lambda p: p["cases"][accepted_case]["runs"][1]["execution"].update(cost=MAX_BUDGET + 1)),
            ("undeclared divergence", lambda p: p["cases"][accepted_case].update(declared_divergence={"backends": ["chialisp"], "outcome": {"status": "rejected"}, "reason": "invented"})),
        ]
    if rejected_case is not None:
        report_mutations += [
            ("rejection without typed failure", lambda p: p["cases"][rejected_case]["runs"][1]["execution"].pop("failure")),
            ("resource failure as rejection", lambda p: p["cases"][rejected_case]["runs"][1]["execution"].update(failure={"kind": "resource", "code": "cost_exceeded"})),
        ]
    if puzzle["source_dependencies"]:
        report_mutations.append(("stale dependency text", lambda p: p["source_dependencies"][0].update(source=p["source_dependencies"][0]["source"] + "\n")))
    if puzzle.get("native_sources"):
        report_mutations.append(("stale native source", lambda p: p["native_sources"][0].update(sha256="0" * 64)))
    if "module_identity" in puzzle:
        report_mutations.append(("false identity flag", lambda p: p["module_identity"]["comparisons"][0].update(
            serialized_bytes_equal=not p["module_identity"]["comparisons"][0]["serialized_bytes_equal"])))
    count = 0
    for label, mutate in report_mutations:
        changed = copy.deepcopy(report)
        mutate(puzzle_row(changed, slug))
        require(json_wire(puzzle_row(changed, slug)) != original_wire, f"{slug}: no-op report mutation: {label}")
        try:
            validate(changed, reader)
        except (ValueError, KeyError, TypeError, IndexError):
            count += 1
            continue
        raise ValueError(f"{slug}: corrupted report evidence passed: {label}")
    overrides = [
        ("changed Rust source", {f"puzzles/{slug}/puzzle.rs": reader(f"puzzles/{slug}/puzzle.rs") + b"\n"}),
        ("changed Chialisp source", {f"puzzles/{slug}/reference.clsp": reader(f"puzzles/{slug}/reference.clsp") + b"\n"}),
        ("changed Rue source", {f"puzzles/{slug}/puzzle.rue": reader(f"puzzles/{slug}/puzzle.rue") + b"\n"}),
    ]
    for mutation in ("case", "order", "claim", "environment", "mode", "dependency", "native", "include", "budget", "classification"):
        bad = copy.deepcopy(manifest)
        if mutation == "case": bad["cases"].pop()
        elif mutation == "order": bad["cases"].reverse()
        elif mutation == "claim": bad["security_scope"] = "Production ready"
        elif mutation == "environment": bad["environment"] = "ignored" if bad.get("environment", "strict") != "ignored" else "strict"
        elif mutation == "mode": bad["reference_mode"] = "expression" if bad.get("reference_mode", "module") == "module" else "module"
        elif mutation == "dependency": bad["source_dependencies"] = bad.get("source_dependencies", []) + ["puzzles/lib.rs"]
        elif mutation == "native": bad["native_sources"] = bad.get("native_sources", []) + ["puzzles/lib.rs"]
        elif mutation == "include": bad["reference_includes"] = {"invented.clib": "puzzles/lib.rs"}
        elif mutation == "budget": bad["validation_max_cost"] = 1 if bad.get("validation_max_cost") != 1 else 2
        else: bad["provenance"]["classification"] = "invented"
        overrides.append((f"manifest {mutation}", {f"puzzles/{slug}/puzzle.json": json.dumps(bad).encode()}))
    pinned = []

    def collect(value):
        if isinstance(value, dict):
            for key in PATH_KEYS:
                if isinstance(value.get(key), str) and any(field in value for field in ("sha256", "source_sha256", "git_blob_sha1", "clvm_hex")):
                    pinned.append(value[key])
            for item in value.values():
                collect(item)
        elif isinstance(value, list):
            for item in value:
                collect(item)

    collect(manifest["provenance"])
    for path in sorted(set(pinned)):
        overrides.append((f"changed pinned file {path}", {path: reader(path) + b"\n"}))
    for label, changes in overrides:
        require(any(reader(path) != value for path, value in changes.items()), f"{slug}: no-op source mutation: {label}")
        patched = lambda path, changes=changes: changes[path] if path in changes else reader(path)
        try:
            validate(report, patched)
        except (ValueError, KeyError, TypeError, IndexError):
            count += 1
            continue
        raise ValueError(f"{slug}: corrupted source/manifest passed: {label}")
    return count
