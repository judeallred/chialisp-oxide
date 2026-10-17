"""Mutation controls for the hash-port gate; no compiler/VM is invoked here."""
import copy
import json
import runpy

import hashing_provenance as provenance
from hashing_vectors import (SLUGS, expected, generated, named, parse, require,
                             serialize, sha_tree, resource_cases, should_exhaust, MODES)


def json_wire(value):
    """Stable JSON bytes preserve bool/int/float distinctions Python equality erases."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")


def expect_rejection(action, label):
    try:
        action()
    except (ValueError, KeyError, TypeError, IndexError):
        return
    raise ValueError("Corrupted hashing evidence passed: " + label)


def source_controls():
    manifests = provenance.sources()
    overrides = []
    for path in [*provenance.PINS, provenance.COMMIT, provenance.TREE_PATH, provenance.SPAN]:
        overrides.append(("changed pinned file " + path, {path: provenance.read_source(path) + b"\n"}))
    overrides += [
        ("changed visible raw adapter", {"puzzles/sha256tree/reference.clsp": provenance.read_source("puzzles/sha256tree/reference.clsp").replace(b"(sha256tree ENV)", b"(sha256tree (f ENV))")}),
        ("changed whole module", {"puzzles/singleton-launcher/reference.clsp": provenance.read_source("puzzles/singleton-launcher/reference.clsp") + b"\n"}),
        ("changed generated input bytes", {"puzzles/sha256tree/generated-cases.json": b"[]\n"}),
    ]
    for slug in SLUGS:
        for mutation in ("classification", "blob", "license", "tree", "source_path", "mode", "environment", "case", "order", "dependencies", "native", "includes", "unknown_claim"):
            bad = copy.deepcopy(manifests[slug])
            p = bad["provenance"]
            if mutation == "classification": p["classification"] = "original_teaching_example"
            elif mutation == "blob": p["upstream"]["git_blob_sha1"] = "0" * 40
            elif mutation == "license": p["license"]["spdx"] = "MIT"
            elif mutation == "tree": p["tree"]["root_tree_sha1"] = "0" * 40
            elif mutation == "source_path": p["upstream"]["source_path"] = "puzzles/fake.clsp"
            elif mutation == "mode": bad["reference_mode"] = "expression"
            elif mutation == "environment": bad["environment"] = "ignored"
            elif mutation == "case": bad["cases"].pop()
            elif mutation == "order": bad["cases"].reverse()
            elif mutation == "dependencies": bad["source_dependencies"].pop()
            elif mutation == "native": bad["native_sources"].pop()
            elif mutation == "includes": bad["reference_includes"] = {"unknown.clib": provenance.INCLUDE}
            else: p["invented_module_hash"] = "00" * 32
            overrides.append((slug + "/" + mutation, {f"puzzles/{slug}/puzzle.json": json.dumps(bad).encode()}))
    for mutation in ("artifact_blob", "canonical_hex", "canonical_hash", "canonical_size", "include", "adapter"):
        bad = copy.deepcopy(manifests["singleton-launcher"])
        p = bad["provenance"]
        if mutation == "artifact_blob": p["upstream_artifact"]["git_blob_sha1"] = "0" * 40
        elif mutation == "canonical_hex": p["upstream_artifact"]["clvm_hex"] = "80"
        elif mutation == "canonical_hash": p["upstream_artifact"]["tree_hash"] = "0" * 64
        elif mutation == "canonical_size": p["upstream_artifact"]["bytes"] = 174
        elif mutation == "include": p["include_dependencies"] = []
        else: p["source_adaptation"]["kind"] = "typed_arguments"
        overrides.append((mutation, {"puzzles/singleton-launcher/puzzle.json": json.dumps(bad).encode()}))
    for mutation in ("missing", "zero", "too_high", "string", "float", "boolean"):
        bad = copy.deepcopy(manifests["sha256tree"])
        if mutation == "missing": del bad["validation_max_cost"]
        else: bad["validation_max_cost"] = {"zero": 0, "too_high": 10_000_001, "string": "10000000", "float": 10_000_000.0, "boolean": True}[mutation]
        overrides.append(("hash comparison budget " + mutation, {"puzzles/sha256tree/puzzle.json": json.dumps(bad).encode()}))
    bad = copy.deepcopy(manifests["singleton-launcher"])
    bad["validation_max_cost"] = 1_000_000
    overrides.append(("launcher omitted default budget", {"puzzles/singleton-launcher/puzzle.json": json.dumps(bad).encode()}))
    for label, changes in overrides:
        require(any(provenance.read_source(path) != value for path, value in changes.items()), "No-op source mutation: " + label)
        reader = lambda path: changes[path] if path in changes else provenance.read_source(path)
        expect_rejection(lambda: provenance.sources(reader), label)
    return len(overrides)


def relation_controls():
    nil_hash = "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"
    zero_hash = "00e146297686edb230b0e85eb3bbbef74d745dcd89d98469db35aaad8d2e08db"
    require(expected("sha256tree", "()") == {"status": "accepted", "output_hex": "a0" + nil_hash}, "Nil tree hash known answer differs")
    require(expected("sha256tree", "0x00d9") == {"status": "accepted", "output_hex": "a0" + zero_hash}, "Leading-zero digest/width known answer differs")
    require(len(named("sha256tree")) == 39 and len(named("singleton-launcher")) == 20 and len(generated()) == 275,
            "Changed fixed coverage cardinality")
    for text in ["()", "0x00", "(1)", "(1 2)", "(1 2 . 0x00)"]:
        require(expected("singleton-launcher", text) == {"status": "rejected"}, "Missing projection must reject")
    # An ignored tail cannot enter the announcement or impose a proper-list ABI.
    same = [expected("singleton-launcher", text) for text in ["(1 2 3)", "(1 2 3 4)", "(1 2 3 . 0x00)"]]
    require(same[0] == same[1] == same[2], "Launcher ignored-tail semantics changed")
    result = expected("singleton-launcher", "((1 . 2) (3 . 4) (5 . 6))")
    fields = parse("((1 . 2) (3 . 4) (5 . 6))")
    first, rest = fields
    second = rest[0]
    wanted_coin = ((b"\x33", (first, (second, b""))))
    require(bytes.fromhex(result["output_hex"]).startswith(b"\xff" + serialize(wanted_coin)), "Launcher narrowed or normalized arbitrary field trees")
    mutations = [
        ("sha256tree", "missing atom domain", {"atom_domain": b""}),
        ("sha256tree", "wrong pair domain", {"pair_domain": b"\x01"}),
        ("sha256tree", "reverse child order", {"swap": True}),
        ("sha256tree", "numeric atom normalization", {"normalize": True}),
        ("sha256tree", "drop leading digest zeros", {"drop_zero": True}),
        ("singleton-launcher", "strict tail instead of ignored", {"strict_tail": True}),
        ("singleton-launcher", "omit committed third field", {"omit_third": True}),
        ("singleton-launcher", "normalize committed padded amount", {"normalize": True}),
    ]
    for slug, label, options in mutations:
        require(any(expected(slug, row["solution"]) != expected(slug, row["solution"], **options) for row in named(slug)),
                "No actual input witnesses relation mutation: " + label)
    # Keep expected data cross-checked with the separate shared codec hash, without
    # using that implementation to produce the policy's expected outcomes.
    for row in named("sha256tree") + generated():
        value = parse(row["solution"])
        require(sha_tree(value) == provenance_hash(value), "Independent postorder hash disagrees with codec")
    return len(mutations) + 8


def provenance_hash(value):
    from hashing_vectors import CODEC
    return CODEC["tree_hash"](value)


def report_controls(report):
    gate = runpy.run_path(str(provenance.ROOT / "scripts/check-hashing-puzzles.py"))
    gate["validate"](report)
    count = 0
    original_wire = json_wire(report)
    for slug in SLUGS:
        mutations = ["missing_case", "reordered_cases", "missing_mode", "reordered_modes", "boolean_mode", "missing_run", "generated",
                     "native_source", "native_source_omitted", "stale_source", "dependency", "include", "artifact_size", "artifact_hash",
                     "coherent_all_outputs", "resource", "unknown_failure", "failure_on_success", "missing_failure", "module_claim", "budget", "over_budget"]
        if slug == "singleton-launcher":
            mutations += ["identity_flag", "canonical_chialisp", "ledger_omitted", "all_language_ledger_omitted"]
        else:
            mutations += ["leading_zero_digest", "padding_normalized", "missing_budget", "missing_controls", "control_case", "control_order",
                          "control_budget", "control_native", "control_policy_rejection", "control_missing_metadata", "control_unknown_code",
                          "control_infrastructure", "control_coherent_outputs", "control_mode"]
        for mutation in mutations:
            bad = copy.deepcopy(report)
            puzzle = next(p for p in bad["puzzles"] if p["slug"] == slug)
            case = next(c for c in puzzle["cases"] if c["native"]["status"] == "accepted")
            execution = case["runs"][0]["execution"]
            if mutation == "budget": puzzle["validation_max_cost"] = 1 if slug == "sha256tree" else 10_000_000
            elif mutation == "over_budget": execution["cost"] = 10_000_001 if slug == "sha256tree" else 1_000_001
            elif mutation == "missing_budget": del puzzle["validation_max_cost"]
            elif mutation == "missing_controls": del puzzle["resource_controls"]
            elif mutation.startswith("control_"):
                controls = puzzle["resource_controls"]
                target = controls["cases"][0]
                low = target["runs"][0]["execution"]
                if mutation == "control_case": controls["cases"].pop()
                elif mutation == "control_order": controls["cases"].reverse()
                elif mutation == "control_budget": controls["max_cost"] = 10_000_000
                elif mutation == "control_native": target["native"]["output_hex"] = "80"
                elif mutation == "control_policy_rejection": low["outcome"]["status"] = "rejected"
                elif mutation == "control_missing_metadata": del low["failure"]
                elif mutation == "control_unknown_code": low["failure"]["code"] = "unknown"
                elif mutation == "control_infrastructure": low["failure"] = {"kind": "infrastructure", "code": "internal_error"}
                elif mutation == "control_mode": target["runs"].reverse()
                else:
                    target["native"]["output_hex"] = "80"
                    for run in target["runs"]:
                        run["execution"] = {"outcome": {"status": "accepted", "output_hex": "80"}, "cost": 1, "error": None}
            elif mutation == "missing_case": puzzle["cases"].pop()
            elif mutation == "reordered_cases": puzzle["cases"].reverse()
            elif mutation == "missing_mode": puzzle["artifacts"].pop()
            elif mutation == "reordered_modes": puzzle["artifacts"].reverse()
            elif mutation == "boolean_mode": puzzle["artifacts"][0]["optimize_lir"] = 0
            elif mutation == "missing_run": case["runs"].pop()
            elif mutation == "generated": puzzle["generated_cases"] -= 1
            elif mutation == "native_source": puzzle["native_sources"][0]["sha256"] = "0" * 64
            elif mutation == "native_source_omitted": puzzle["native_sources"].pop()
            elif mutation == "stale_source": puzzle["source"] += "\n"
            elif mutation == "dependency": puzzle["source_dependencies"][0]["source"] += "\n"
            elif mutation == "include": puzzle["reference_includes"] = {"fake.clib": provenance.INCLUDE}
            elif mutation == "artifact_size": puzzle["artifacts"][0]["bytes"] += 1
            elif mutation == "artifact_hash": puzzle["artifacts"][0]["tree_hash"] = "0" * 64
            elif mutation == "coherent_all_outputs":
                case["native"] = {"status": "accepted", "output_hex": "80"}
                for row in case["runs"]:
                    row["execution"] = {"outcome": copy.deepcopy(case["native"]), "cost": 1, "error": None}
            elif mutation == "resource":
                execution.update(outcome={"status": "rejected"}, cost=None, error="Raise(spoofed)", failure={"kind": "resource", "code": "cost_exceeded"})
            elif mutation == "unknown_failure":
                execution.update(outcome={"status": "rejected"}, cost=None, error="unknown", failure={"kind": "program", "code": "unknown_code"})
            elif mutation == "failure_on_success": execution["failure"] = {"kind": "program", "code": "raise"}
            elif mutation == "missing_failure": execution.update(outcome={"status": "rejected"}, cost=None, error="Raise(spoofed)")
            elif mutation == "module_claim": puzzle["module_identity"] = {"upstream_hex": "80"}
            elif mutation == "identity_flag": puzzle["module_identity"]["comparisons"][0]["serialized_bytes_equal"] = not puzzle["module_identity"]["comparisons"][0]["serialized_bytes_equal"]
            elif mutation == "canonical_chialisp":
                a = puzzle["artifacts"][2]
                a.update(clvm_hex="80", bytes=1, tree_hash=provenance_hash(b"").hex())
            elif mutation == "ledger_omitted": puzzle["simulator"] = []
            elif mutation == "all_language_ledger_omitted":
                puzzle["simulator"].pop()
                for row in puzzle["simulator_comparisons"]: row["scenarios"].pop()
            else:
                label = "leading-zero digest keeps 32-byte output" if mutation == "leading_zero_digest" else "two zero bytes preserve padding"
                target = next(c for c in puzzle["cases"] if c["label"] == label)
                output = target["native"]["output_hex"]
                output = "9f" + output[4:] if mutation == "leading_zero_digest" else expected(slug, "()")['output_hex']
                target["native"]["output_hex"] = output
                for row in target["runs"]: row["execution"]["outcome"]["output_hex"] = output
            require(json_wire(bad) != original_wire, "No-op report corruption: " + mutation)
            expect_rejection(lambda: gate["validate"](bad), slug + "/" + mutation)
            count += 1
    # Numeric 0/1 and 0.0/1.0 compare equal to booleans in Python, but are
    # different JSON wire types. Cover both truth values across every mode.
    original = next(p for p in report["puzzles"] if p["slug"] == "singleton-launcher")
    for index, row in enumerate(original["module_identity"]["comparisons"]):
        for field in ("optimize_lir", "serialized_bytes_equal", "tree_hash_equal"):
            for numeric in (int, float):
                bad = copy.deepcopy(report)
                puzzle = next(p for p in bad["puzzles"] if p["slug"] == "singleton-launcher")
                puzzle["module_identity"]["comparisons"][index][field] = numeric(row[field])
                label = f"identity/{index}/{field}/{numeric.__name__}"
                require(json_wire(bad) != original_wire, "No-op report corruption: " + label)
                expect_rejection(lambda: gate["validate"](bad), label)
                count += 1
    return count


def resource_fixture():
    cases = []
    for index, row in enumerate(resource_cases()):
        desired = expected("sha256tree", row["solution"])
        runs = []
        for backend, optimize in MODES:
            execution = ({"outcome": {"status": "inconclusive"}, "cost": None, "error": "synthetic test diagnostic",
                          "failure": {"kind": "resource", "code": "cost_exceeded"}}
                         if should_exhaust(backend, optimize, index)
                         else {"outcome": copy.deepcopy(desired), "cost": 1, "error": None})
            runs.append({"backend": backend, "optimize_lir": optimize, "execution": execution})
        cases.append({**row, "native": desired, "runs": runs})
    return {"slug": "sha256tree", "resource_controls": {"max_cost": 1_000_000, "cases": cases}}


def self_test(report=None):
    count = source_controls() + relation_controls()
    if report is not None:
        count += report_controls(report)
        count += refined_resource_controls(report)
    return count


def refined_resource_controls(report):
    """Only the three reviewed optimized Rust diagnostics may improve."""
    from runtime_cost_migration import is_new, rust_on
    if not is_new(report):
        return 0
    gate = runpy.run_path(str(provenance.ROOT / "scripts/check-hashing-puzzles.py"))
    gate["validate"](report)
    count = 0
    for mutation in ("old_pipeline", "unknown_pipeline", "on_old_resource_2", "on_old_resource_3", "on_old_resource_4",
                     "on_wrong_digest", "on_program_rejection", "off_unreviewed_success", "on_wrong_case_resource", "numeric_on_mode"):
        bad = copy.deepcopy(report)
        puzzle = next(p for p in bad["puzzles"] if p["slug"] == "sha256tree")
        cases = puzzle["resource_controls"]["cases"]
        if mutation == "old_pipeline":
            from runtime_cost_migration import OLD_PIPELINE
            bad["optimization_pipeline"] = copy.deepcopy(OLD_PIPELINE)
        elif mutation == "unknown_pipeline": bad["optimization_pipeline"]["rust_mir"]["optimized"].append("unknown")
        elif mutation.startswith("on_old_resource_") or mutation == "on_wrong_case_resource":
            index = int(mutation[-1]) if mutation.startswith("on_old_resource_") else 0
            rust_on(cases[index]["runs"])["execution"] = {"outcome": {"status": "inconclusive"}, "cost": None,
                "error": "CostExceeded", "failure": {"kind": "resource", "code": "cost_exceeded"}}
        elif mutation == "on_wrong_digest": rust_on(cases[2]["runs"])["execution"]["outcome"]["output_hex"] = "80"
        elif mutation == "on_program_rejection":
            rust_on(cases[2]["runs"])["execution"] = {"outcome": {"status": "rejected"}, "cost": None,
                "error": "Raise(spoofed)", "failure": {"kind": "program", "code": "raise"}}
        elif mutation == "off_unreviewed_success":
            cases[2]["runs"][0]["execution"] = {"outcome": copy.deepcopy(cases[2]["native"]), "cost": 1, "error": None}
        else: rust_on(cases[2]["runs"])["optimize_lir"] = 1
        require(json_wire(bad) != json_wire(report), "No-op refined resource control: " + mutation)
        expect_rejection(lambda: gate["validate"](bad), mutation)
        count += 1
    return count
