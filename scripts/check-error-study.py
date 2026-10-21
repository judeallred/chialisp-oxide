#!/usr/bin/env python3
"""Execute intentionally weak tests, missed witnesses and real Rust diagnostics.

Default: reproduce and compare corpus/error-study.json. --write publishes a
fresh report. --check (also --freshness-only) checks declared inputs and recorded claims without
executing compilers; --self-test exercises negative controls for that gate.
No generated Rust policy, workspace manifest edits or network access are used.
"""

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
import unittest

from cargo_authoring import contained_file, load_json
from source_walk import source_files
from vm_failures import PROTOCOL, check_execution, execution_sources


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = "experiments/error-study/studies.json"
OUTPUT = ROOT / "corpus/error-study.json"
EXPECTED_ERRORS = {"domain-swap": "E0308", "branch-shape": "E0308", "integer-truthiness": "E0308", "unresolved-name": "E0425",
                   "unbounded-growth": None, "list-truthiness": "E0308", "role-swap": "E0308", "padded-atom": None,
                   "arity-mismatch": "E0061", "missing-cap": None, "missing-authorization": None}
CATEGORIES = {"domain-swap": "rust_typecheck", "branch-shape": "rust_typecheck", "integer-truthiness": "rust_typecheck",
              "unresolved-name": "rust_name_resolution", "unbounded-growth": "rust_runtime_check", "list-truthiness": "rust_typecheck",
              "role-swap": "rust_typecheck", "padded-atom": "runtime_abi_rejection", "arity-mismatch": "rust_typecheck",
              "missing-cap": "runtime_policy_control", "missing-authorization": "runtime_policy_control"}
LAYERS = {"native_rust_typecheck", "native_rust_name_resolution", "native_rust_overflow_check", "compiled_checked_arithmetic", "compiled_strict_abi"}
# Studies whose compiled layer is an executed corpus case: the independent
# Chialisp module accepts the witness and the Rust CLVM rejects it in both modes.
CORPUS_REFERENCES = {"unbounded-growth": ("module-add", "overflow rejects in Rust; the original grows"),
                     "padded-atom": ("module-add", "padded atom rejects in Rust; the original reads it as one"),
                     "arity-mismatch": ("module-add", "extra argument ignored by the original")}
# Native test names for studies whose faulty Rust compiles: the limited suite
# passes; the witness fails (101) for policy controls or passes (0) when a
# native runtime check rejects the witness.
NATIVE_TESTS = {"missing-cap": ("ordinary_small_values_pass", "independent_output_cap_catches_the_missing_guard", 101),
                "missing-authorization": ("small_payments_pass", "large_payment_requires_a_signature", 101),
                "unbounded-growth": ("ordinary_deposits_pass", "overflowing_deposit_is_rejected", 0)}
RUE_DIAGNOSTICS = {"branch-shape": "Cannot assign `(CreateCoin, nil) | CreateCoin` to `List<Condition>`",
                   "integer-truthiness": "Cannot compare `Int` to `Bool` without a type guard",
                   "unresolved-name": "Undeclared symbol `amout`",
                   "list-truthiness": "Cannot compare `Any` to `Bool` without a type guard"}
CORPUS_REPORT = ROOT / "corpus/validation.json"
LIBRARIES = {"chialisp_oxide", "chialisp_oxide_reference", "chialisp_oxide_backend_experiment",
             "chialisp_oxide_rue", "serde_json"}
DEPENDENCIES = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
                "crates/chialisp-oxide/Cargo.toml", "crates/chialisp-oxide/src/lib.rs",
                "crates/chialisp-oxide/src/domain.rs", "crates/chialisp-oxide/src/tree.rs", "crates/chialisp-oxide-macros/Cargo.toml",
                "crates/chialisp-oxide-macros/src/lib.rs", "crates/chialisp-oxide-reference/Cargo.toml",
                "crates/chialisp-oxide-reference/src/lib.rs", "crates/chialisp-oxide-rue/Cargo.toml",
                "crates/chialisp-oxide-rue/src/lib.rs", "experiments/backend/Cargo.toml",
                "experiments/backend/src/lib.rs", "vendor/rue-lir/Cargo.toml",
                "scripts/check-error-study.py", "scripts/cargo_authoring.py",
                "scripts/source_walk.py", "scripts/test_source_walk.py"]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs():
    paths = set(DEPENDENCIES)
    paths.update(row["path"] for row in execution_sources())
    # Share traversal budgets across both roots while preserving every regular
    # fixture extension and the vendor's existing Rust-only input contract.
    discovered = source_files(ROOT, ["experiments/error-study", "vendor/rue-lir/src"], suffixes=None)
    paths.update(path for path in discovered
                 if path.startswith("experiments/error-study/") or path.endswith(".rs"))
    return [{"path": path, "sha256": sha256(contained_file(ROOT, path))} for path in sorted(paths)]


def specification():
    manifest = load_json(ROOT / MANIFEST)
    require(manifest["schema_version"] == 1, "Unsupported error-study fixture schema")
    rows = manifest["studies"]
    require([row["id"] for row in rows] == list(EXPECTED_ERRORS), "Reviewed study set/order changed")
    for row in rows:
        require(row["rust_error"] == EXPECTED_ERRORS[row["id"]], "Reviewed native compiler expectation changed")
        require(row["category"] == CATEGORIES[row["id"]], "Reviewed study category changed")
        require(row["weak_suite"] and row["limitations"] and row["claim"], "Missing study assertions or scope")
        require(isinstance(row["caught_by"], list) and set(row["caught_by"]) <= LAYERS and len(set(row["caught_by"])) == len(row["caught_by"])
                and (bool(row["caught_by"]) == (row["category"] != "runtime_policy_control")), "Each study must name the actual layers that catch it")
        reference = row.get("corpus_reference")
        expected_reference = CORPUS_REFERENCES.get(row["id"])
        require((reference is None) == (expected_reference is None)
                and (reference is None or reference == {"slug": expected_reference[0], "case": expected_reference[1]}), "Reviewed corpus reference changed")
        tests = row.get("native_tests")
        expected_tests = NATIVE_TESTS.get(row["id"])
        require((tests is None) == (expected_tests is None)
                and (tests is None or tests == {"limited": expected_tests[0], "witness": expected_tests[1], "witness_exit_code": expected_tests[2]}),
                "Reviewed native test names changed")
        required_sources = {"chialisp", "rust_bad", "rust_fixed", "rue"}
        if row["id"] == "domain-swap":
            required_sources.add("rue_nominal_control")
        require(set(row["sources"]) == required_sources, "Missing language fixture")
        for path in row["sources"].values():
            require(path.startswith(f"experiments/error-study/{row['id']}/"), "Source belongs to a different study")
            contained_file(ROOT, path)
    return manifest


def corpus_case(row):
    """The executed corpus case a study cites for its compiled layer, read from the current report."""
    reference = row["corpus_reference"]
    report = load_json(CORPUS_REPORT)
    puzzles = [puzzle for puzzle in report["puzzles"] if puzzle["slug"] == reference["slug"]]
    require(len(puzzles) == 1 and puzzles[0]["validated"] is True, f"{row['id']}: referenced corpus puzzle is missing")
    cases = [case for case in puzzles[0]["cases"] if case["label"] == reference["case"]]
    require(len(cases) == 1, f"{row['id']}: referenced corpus case is missing")
    case = cases[0]
    divergence = case.get("declared_divergence")
    require(divergence is not None and "chialisp" in divergence["backends"] and divergence["outcome"]["status"] == "accepted"
            and case["native"] == {"status": "rejected"}, f"{row['id']}: referenced case must be a declared Chialisp acceptance that native Rust rejects")
    runs = [{"backend": run["backend"], "optimize_lir": run["optimize_lir"], "outcome": run["execution"]["outcome"]} for run in case["runs"]]
    require({(run["backend"], run["optimize_lir"]) for run in runs} == {("rust_mir", False), ("rust_mir", True), ("chialisp", True), ("rue_source", False), ("rue_source", True)}
            and all(run["outcome"] == {"status": "rejected"} for run in runs if run["backend"] == "rust_mir")
            and all(run["outcome"] == divergence["outcome"] for run in runs if run["backend"] == "chialisp"),
            f"{row['id']}: referenced case must show Rust CLVM rejecting in both modes while Chialisp accepts")
    return {"slug": reference["slug"], "case": reference["case"], "solution": case["solution"], "native": case["native"],
            "declared_divergence": divergence, "runs": runs,
            "scope": "Executed corpus observation of the compiled layer; finite evidence, not a compiler-preservation proof."}


def atom(payload):
    size = len(payload)
    require(size < 64, "Study atom encoder deliberately supports fewer than 64 bytes")
    if size == 0:
        return b"\x80"
    if size == 1 and payload[0] < 128:
        return payload
    return bytes([128 + size]) + payload


def wire(value):
    """Independent small CLVM codec for explicitly stated expected result trees."""
    if type(value) is int:
        if value == 0:
            return atom(b"")
        size = 1
        while not -(1 << (8 * size - 1)) <= value < (1 << (8 * size - 1)):
            size += 1
        return atom(value.to_bytes(size, "big", signed=True))
    if isinstance(value, dict):
        require(set(value) == {"bytes"}, "Expected explicit byte atom")
        return atom(bytes.fromhex(value["bytes"]))
    require(isinstance(value, list), "Expected integer, explicit byte atom or proper list")
    result = b"\x80"
    for item in reversed(value):
        result = b"\xff" + wire(item) + result
    return result


def expected(case):
    outcome = case["expected"]
    if outcome == {"status": "rejected"}:
        return outcome
    require(set(outcome) == {"status", "tree"} and outcome["status"] == "accepted", "Invalid independent expected result")
    return {"status": "accepted", "output_hex": wire(outcome["tree"]).hex()}


def normalized(text):
    text = text.replace(str(ROOT), "<repo>")
    return re.sub(r"(thread '[^'\n]+') \([0-9]+\)", r"\1 (<thread>)", text)


def command(args, success=0, timeout=180):
    environment = os.environ.copy()
    for key in ("RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTUP_TOOLCHAIN", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"):
        environment.pop(key, None)
    environment["RUST_BACKTRACE"] = "0"
    result = subprocess.run([str(arg) for arg in args], cwd=ROOT, env=environment,
                            capture_output=True, text=True, timeout=timeout, check=False)
    if success is not None:
        require(result.returncode == success,
                f"Command exited {result.returncode}, expected {success}: {args}\n{result.stdout}\n{result.stderr}")
    return result


def compiler_and_libraries():
    toolchain = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    rustc = command(["rustup", "which", "--toolchain", toolchain, "rustc"]).stdout.strip()
    version = command([rustc, "--version"]).stdout.strip()
    build = command(["cargo", f"+{toolchain}", "build", "--locked", "--offline", "--target-dir", ROOT / "target",
                     "-p", "chialisp-oxide-cli", "--message-format=json"])
    libraries = {}
    for line in build.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") != "compiler-artifact":
            continue
        name = message["target"]["name"]
        if name not in LIBRARIES:
            continue
        artifacts = [Path(path) for path in message["filenames"] if path.endswith(".rlib")]
        if artifacts:
            require(len(artifacts) == 1, f"Ambiguous native dependency artifact: {name}")
            libraries.setdefault(name, []).append(artifacts[0])
    require(set(libraries) == LIBRARIES, "Build did not identify every actual study dependency")
    # Cargo can emit host and target serde_json units with identical features.
    # Let rustc check which unit implements the actual backend's Serialize trait.
    for name in LIBRARIES - {"serde_json"}:
        require(len(libraries[name]) == 1, f"Ambiguous study library artifact: {name}")
        libraries[name] = libraries[name][0]
    require(1 <= len(libraries["serde_json"]) <= 8, "Unexpected serde_json build-unit count")
    return rustc, version, libraries


def rust_command(rustc, source, directory, libraries, test=False):
    output = directory / (source.parent.name.replace("-", "_") + ("_test" if test else ".rmeta"))
    args = [rustc, source, "--edition=2024", "--color=never", "--crate-name", "error_study",
            "-C", "overflow-checks=yes", "-L", f"dependency={ROOT / 'target/debug/deps'}",
            "--extern", f"chialisp_oxide={libraries['chialisp_oxide']}", "-o", output]
    args += ["--test"] if test else ["--crate-type=lib", "--emit=metadata", "--error-format=json"]
    result = command(args, success=None)
    errors = []
    if not test:
        for line in result.stderr.splitlines():
            diagnostic = json.loads(line)
            if diagnostic.get("level") == "error" and diagnostic.get("code"):
                errors.append({"code": diagnostic["code"]["code"], "message": diagnostic["message"],
                               "rendered": normalized(diagnostic["rendered"])})
    return result, output, errors


def test_binary(path, name=None, success=0):
    args = [path, "--format=json", "-Zunstable-options", "--test-threads=1"]
    if name:
        args += [name, "--exact"]
    result = command(args, success=success, timeout=30)
    tests = []
    for line in result.stdout.splitlines():
        event = json.loads(line)
        if event["type"] == "test" and event["event"] in ("ok", "failed", "ignored"):
            tests.append({key: normalized(event[key]) for key in ("name", "event", "stdout") if key in event})
    require(tests, "Native test binary did not execute any tests")
    return {"exit_code": result.returncode, "tests": tests}


def executions(bridge, language, row, source_key=None):
    cases = row["weak_suite"] + [row["missed_witness"]]
    result = command([bridge, language, ROOT / row["sources"][source_key or language], *[case["input"] for case in cases]], timeout=60)
    modes = json.loads(result.stdout)
    for mode in modes:
        if mode["status"] == "compile_rejected":
            mode["diagnostic"] = normalized(mode["diagnostic"])
            continue
        runs = mode.pop("runs")
        require(len(runs) == len(cases), "VM observation count differs from specified cases")
        observations = []
        for case, run in zip(cases, runs):
            require(case["input"] == run["input"], "VM input ordering mismatch")
            execution = run["execution"]
            check_execution(execution, f"{language}/{case['name']}")
            observations.append({"name": case["name"], "input": case["input"], "expected": expected(case),
                                 "execution": execution, "matches_expected": execution["outcome"] == expected(case)})
        mode["weak_suite"], mode["missed_witness"] = observations[:-1], observations[-1]
    return modes


def reproduce():
    before = inputs()
    manifest = specification()
    rustc, version, libraries = compiler_and_libraries()
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-error-study-") as temporary:
        directory = Path(temporary)
        bridge = directory / "bridge"
        args = [rustc, ROOT / "experiments/error-study/bridge.rs", "--edition=2024", "-o", bridge,
                "-L", f"dependency={ROOT / 'target/debug/deps'}"]
        for name in sorted(LIBRARIES - {"chialisp_oxide", "serde_json"}):
            args += ["--extern", f"{name}={libraries[name]}"]
        failures = []
        for serialization in sorted(libraries["serde_json"]):
            attempt = command(args + ["--extern", f"serde_json={serialization}"], success=None)
            if attempt.returncode == 0:
                break
            failures.append(attempt.stderr)
        else:
            raise ValueError("No Cargo serde_json unit typechecks with the study bridge:\n" + "\n".join(failures))
        versions = json.loads(command([bridge, "versions"]).stdout)
        studies = []
        for row in manifest["studies"]:
            failed, _, errors = rust_command(rustc, ROOT / row["sources"]["rust_bad"], directory, libraries)
            rust = {"compilation": {"status": "accepted" if failed.returncode == 0 else "rejected", "diagnostics": errors}}
            fixed, test, _ = rust_command(rustc, ROOT / row["sources"]["rust_fixed"], directory, libraries, test=True)
            require(fixed.returncode == 0, f"Corrected Rust failed to compile: {fixed.stderr}")
            rust["corrected_native"] = test_binary(test)
            if row["id"] in NATIVE_TESTS:
                limited, witness, exit_code = NATIVE_TESTS[row["id"]]
                bad, test, _ = rust_command(rustc, ROOT / row["sources"]["rust_bad"], directory, libraries, test=True)
                require(bad.returncode == 0, f"Compiling faulty Rust with native tests failed: {bad.stderr}")
                rust["limited_native_suite"] = test_binary(test, limited)
                rust["expanded_native_witness"] = test_binary(test, witness, success=exit_code)
            study = {"id": row["id"], "title": row["title"], "category": row["category"], "caught_by": row["caught_by"],
                     "claim": row["claim"], "limitations": row["limitations"], "sources": row["sources"],
                     "chialisp": executions(bridge, "chialisp", row), "rust": rust,
                     "rue": executions(bridge, "rue", row)}
            if row["id"] in CORPUS_REFERENCES:
                study["corpus_evidence"] = corpus_case(row)
            studies.append(study)
            if row["id"] == "domain-swap":
                studies[-1]["rue_nominal_control"] = executions(bridge, "rue", row, "rue_nominal_control")
    require(before == inputs(), "Declared study inputs changed during execution")
    report = {"schema_version": 1, "scope": manifest["scope"], "toolchain": {"rustc": version, **versions},
              "execution_vm": "clvmr 0.21.0 MEMPOOL_MODE", "vm_failure_protocol": PROTOCOL, "sources": before, "studies": studies}
    validate_report(report)
    return report


def boolean_modes(rows, label):
    require(isinstance(rows, list) and all(isinstance(row, dict) and type(row.get("optimize_lir")) is bool
                                         for row in rows), f"{label}: compiler modes must be actual booleans")
    for row in rows:
        if row.get("status") == "compiled":
            require(isinstance(row.get("artifact"), dict) and type(row["artifact"].get("optimize_lir")) is bool,
                    f"{label}: artifact mode must be an actual boolean")


def validate_report(report):
    require(report.get("vm_failure_protocol") == PROTOCOL, "Missing/unsupported VM failure protocol")
    manifest = specification()
    require(type(report["schema_version"]) is int and report["schema_version"] == 1, "Unexpected study report schema")
    require(report["sources"] == inputs(), "Stale declared study source/dependency hashes")
    require(report["scope"] == manifest["scope"] and report["execution_vm"] == "clvmr 0.21.0 MEMPOOL_MODE", "Changed study claim scope/VM")
    require(report["toolchain"]["reference"] == "chialisp 0.4.6"
            and report["toolchain"]["rue_revision"] == "245c107a8e795229ea08097142f005a4c0d91075"
            and report["toolchain"]["rustc"].startswith("rustc 1.99.0-nightly"), "Unexpected study compiler pins")
    require(len(report["studies"]) == len(manifest["studies"]), "Missing study evidence")
    for row, evidence in zip(manifest["studies"], report["studies"]):
        for key in ("id", "title", "category", "caught_by", "claim", "limitations", "sources"):
            require(evidence[key] == row[key], f"{row['id']}: changed study identity/scope")
        if row["id"] in CORPUS_REFERENCES:
            require(evidence["corpus_evidence"] == corpus_case(row), f"{row['id']}: recorded corpus evidence differs from the current executed report")
        else:
            require("corpus_evidence" not in evidence, f"{row['id']}: unexpected corpus reference")
        for language in ("chialisp", "rue", "rue_nominal_control"):
            if language in evidence:
                boolean_modes(evidence[language], f"{row['id']}/{language}")
        require(len(evidence["chialisp"]) == 1 and evidence["chialisp"][0]["status"] == "compiled",
                f"{row['id']}: faulty Chialisp must actually compile")
        require({mode["optimize_lir"] for mode in evidence["rue"]} == {False, True}
                and len(evidence["rue"]) == 2, "Rue outcomes require actual attempts in both optimization modes")
        if row["id"] == "domain-swap":
            control = evidence["rue_nominal_control"]
            require(len(control) == 2 and {mode["optimize_lir"] for mode in control} == {False, True}
                    and all(mode["status"] == "compile_rejected" and "StudyCoinId" in mode["diagnostic"]
                            and "StudyPuzzleHash" in mode["diagnostic"] and "Cannot assign" in mode["diagnostic"] for mode in control),
                    "Rue nominal-domain control must actually reject the direct domain swap")
        for language in ("chialisp", "rue"):
            for mode in evidence[language]:
                require(mode["status"] in ("compiled", "compile_rejected"), "Unsupported language outcome")
                if mode["status"] == "compile_rejected":
                    require(language == "rue" and row["id"] in RUE_DIAGNOSTICS
                            and RUE_DIAGNOSTICS[row["id"]] in mode["diagnostic"],
                            "Missing the actual reviewed Rue type/name diagnostic")
                    continue
                require(language != "rue" or row["id"] not in RUE_DIAGNOSTICS,
                        "Rue unexpectedly accepted a reviewed type/name error")
                require(mode["artifact"]["backend"] == language
                        and mode["artifact"]["optimize_lir"] == mode["optimize_lir"]
                        and type(mode["artifact"]["bytes"]) is int and mode["artifact"]["bytes"] > 0
                        and mode["artifact"]["bytes"] == len(bytes.fromhex(mode["artifact"]["clvm_hex"]))
                        and re.fullmatch(r"[0-9a-f]{64}", mode["artifact"]["tree_hash"]), "Malformed artifact evidence")
                require(len(mode["weak_suite"]) == len(row["weak_suite"]), "Missing weak-test observations")
                for case, observation in zip(row["weak_suite"] + [row["missed_witness"]],
                                             mode["weak_suite"] + [mode["missed_witness"]]):
                    require(observation["name"] == case["name"] and observation["input"] == case["input"]
                            and observation["expected"] == expected(case), "Changed independent test expectation/input")
                    execution = observation["execution"]
                    check_execution(execution, f"{row['id']}/{language}/{case['name']}")
                    require(type(observation["matches_expected"]) is bool
                            and observation["matches_expected"] == (execution["outcome"] == expected(case)), "False test-result claim")
                require(all(case["matches_expected"] for case in mode["weak_suite"]), "The deliberately limited compiled tests must pass")
                require(mode["missed_witness"]["matches_expected"] is False, "The missed compiled policy failure must be observed")
        rust = evidence["rust"]
        if row["rust_error"]:
            require(rust["compilation"]["status"] == "rejected"
                    and any(error["code"] == row["rust_error"] and error["rendered"] for error in rust["compilation"]["diagnostics"]),
                    "Missing actual native Rust compile-fail evidence")
        else:
            require(rust["compilation"] == {"status": "accepted", "diagnostics": []}, "Faulty Rust without a compile error must remain well typed")
            if row["id"] in NATIVE_TESTS:
                limited, witness, exit_code = NATIVE_TESTS[row["id"]]
                require(rust["limited_native_suite"]["exit_code"] == 0 and [test["name"] for test in rust["limited_native_suite"]["tests"]] == [limited]
                        and all(test["event"] == "ok" for test in rust["limited_native_suite"]["tests"]), "Missing passing weak native suite")
                observed = rust["expanded_native_witness"]
                require(observed["exit_code"] == exit_code and [test["name"] for test in observed["tests"]] == [witness], "Missing native witness execution")
                if exit_code == 0:
                    require(row["category"] == "rust_runtime_check" and all(test["event"] == "ok" for test in observed["tests"]),
                            "A native runtime check must actually reject the witness")
                else:
                    require(row["category"] == "runtime_policy_control" and any(test["event"] == "failed" for test in observed["tests"]),
                            "Missing native policy counterexample")
            else:
                require(row["category"] == "runtime_abi_rejection" and "limited_native_suite" not in rust and "expanded_native_witness" not in rust,
                        "Only the runtime ABI study may omit native witness tests")
        corrected = rust["corrected_native"]
        require(corrected["exit_code"] == 0 and corrected["tests"]
                and all(test["event"] == "ok" for test in corrected["tests"]), "Corrected native tests did not pass")


class EvidenceControls(unittest.TestCase):
    def test_compiler_modes_require_actual_boolean_wire_types(self):
        # Include rejected source attempts: they also record a compiler mode.
        original = [{"optimize_lir": flag, "status": status, **({"artifact": {"optimize_lir": flag}} if status == "compiled" else {})}
                    for flag, status in [(False, "compiled"), (True, "compiled"), (False, "compile_rejected"), (True, "compile_rejected")]]
        boolean_modes(original, "synthetic valid mode envelopes")
        for index, row in enumerate(original):
            for numeric in (int, float):
                for target in (["mode", "artifact"] if row["status"] == "compiled" else ["mode"]):
                    changed = copy.deepcopy(original)
                    field = changed[index] if target == "mode" else changed[index]["artifact"]
                    field["optimize_lir"] = numeric(row["optimize_lir"])
                    with self.subTest(index=index, numeric=numeric.__name__, target=target), self.assertRaises(ValueError):
                        boolean_modes(changed, "numeric mode corruption")

    def test_false_claims_and_stale_evidence_are_rejected(self):
        original = load_json(OUTPUT)
        validate_report(original)
        by_id = {study["id"]: study for study in original["studies"]}
        for mutation in ("source", "weak_result", "weak_actual", "witness", "expectation", "rust_code", "rust_accepted",
                         "fixed_failure", "negative_control", "missing_rue", "rue_parse_error", "rue_nominal", "scope",
                         "layer", "corpus_slug", "corpus_rust_outcome", "corpus_dropped", "runtime_check_failed",
                         "control_witness_passed", "abi_native_witness", "second_control_rejected"):
            changed = copy.deepcopy(original)
            row = changed["studies"][0]
            mode = row["chialisp"][0]
            changed_by_id = {study["id"]: study for study in changed["studies"]}
            if mutation == "layer":
                changed_by_id["padded-atom"]["caught_by"] = ["native_rust_typecheck"]
            elif mutation == "corpus_slug":
                changed_by_id["padded-atom"]["corpus_evidence"]["slug"] = "module-times-ten"
            elif mutation == "corpus_rust_outcome":
                run = next(run for run in changed_by_id["arity-mismatch"]["corpus_evidence"]["runs"] if run["backend"] == "rust_mir")
                run["outcome"] = {"status": "accepted", "output_hex": "03"}
            elif mutation == "corpus_dropped":
                del changed_by_id["unbounded-growth"]["corpus_evidence"]
            elif mutation == "runtime_check_failed":
                changed_by_id["unbounded-growth"]["rust"]["expanded_native_witness"]["tests"][0]["event"] = "failed"
            elif mutation == "control_witness_passed":
                witness = changed_by_id["missing-authorization"]["rust"]["expanded_native_witness"]
                witness["exit_code"] = 0
                witness["tests"][0]["event"] = "ok"
            elif mutation == "abi_native_witness":
                changed_by_id["padded-atom"]["rust"]["expanded_native_witness"] = copy.deepcopy(by_id["missing-cap"]["rust"]["expanded_native_witness"])
            elif mutation == "second_control_rejected":
                changed_by_id["missing-authorization"]["rust"]["compilation"] = {"status": "rejected", "diagnostics": []}
            elif mutation == "source":
                changed["sources"][0]["sha256"] = "0" * 64
            elif mutation == "weak_result":
                mode["weak_suite"][0]["matches_expected"] = False
            elif mutation == "weak_actual":
                mode["weak_suite"][0]["execution"]["outcome"]["output_hex"] = "80"
            elif mutation == "witness":
                mode["missed_witness"]["matches_expected"] = True
            elif mutation == "expectation":
                mode["missed_witness"]["expected"] = mode["missed_witness"]["execution"]["outcome"]
            elif mutation == "rust_code":
                row["rust"]["compilation"]["diagnostics"][0]["code"] = "E0000"
            elif mutation == "rust_accepted":
                row["rust"]["compilation"]["status"] = "accepted"
            elif mutation == "fixed_failure":
                row["rust"]["corrected_native"]["tests"][0]["event"] = "failed"
            elif mutation == "negative_control":
                changed["studies"][-1]["rust"]["compilation"]["status"] = "rejected"
            elif mutation == "missing_rue":
                row["rue"].pop()
            elif mutation == "rue_parse_error":
                changed["studies"][1]["rue"][0]["diagnostic"] = "Expected `)`, found identifier"
            elif mutation == "rue_nominal":
                row["rue_nominal_control"][0]["status"] = "compiled"
            else:
                row["claim"] = "Rust proves every puzzle secure"
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                validate_report(changed)

    def test_typed_failure_corruptions_cannot_be_test_results(self):
        original = load_json(OUTPUT)
        validate_report(original)
        # Preserve the recorded outcome and matches_expected flag while corrupting
        # only metadata; these controls must fail at the VM wire boundary.
        for mutation in ("missing", "unknown", "resource", "infrastructure", "inconclusive"):
            changed = copy.deepcopy(original)
            rejected = next(observation["execution"] for study in changed["studies"]
                            for language in ("chialisp", "rue") for mode in study[language]
                            if mode["status"] == "compiled"
                            for observation in mode["weak_suite"] + [mode["missed_witness"]]
                            if observation["execution"]["outcome"]["status"] == "rejected")
            if mutation == "missing": del rejected["failure"]
            elif mutation == "unknown": rejected["failure"]["code"] = "future_code"
            elif mutation == "infrastructure": rejected["failure"] = {"kind": "infrastructure", "code": "internal_error"}
            else:
                rejected["failure"] = {"kind": "resource", "code": "cost_exceeded"}
                if mutation == "inconclusive": rejected["outcome"]["status"] = "inconclusive"
            rejected["error"] = "Raise(spoofed diagnostic)"
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                validate_report(changed)

    def test_independent_codec_boundary_vectors(self):
        for value, encoded in [(0, "80"), (1, "01"), (127, "7f"), (128, "820080"),
                               (-1, "81ff"), (-128, "8180"), (-129, "82ff7f"), ([1, 2, 3], "ff01ff02ff0380")]:
            with self.subTest(value=value):
                self.assertEqual(wire(value).hex(), encoded)

    def test_native_panic_thread_ids_are_normalized(self):
        self.assertEqual(normalized("thread 'policy' (123456) panicked at " + str(ROOT) + "/bad.rs:4:5"),
                         "thread 'policy' (<thread>) panicked at <repo>/bad.rs:4:5")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    parser.add_argument("--write", action="store_true", help="execute and replace the intentional reproducible report")
    parser.add_argument("--freshness-only", "--check", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(EvidenceControls))
        return 0 if result.wasSuccessful() else 1
    try:
        require(not (args.write and args.freshness_only), "--write and --freshness-only are mutually exclusive")
        if args.freshness_only:
            validate_report(load_json(args.output))
        else:
            report = reproduce()
            if args.write:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(report, indent=2) + "\n")
            else:
                require(load_json(args.output) == report, "Executed error-study evidence changed; review and regenerate with --write")
    except (ValueError, KeyError, TypeError, OSError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"Error study failed: {error}\n")
    scope = "declared-source freshness/recorded claims" if args.freshness_only else "actual Chialisp/Rue compilation+VM, rustc diagnostics and native tests"
    manifest = load_json(ROOT / MANIFEST)["studies"]
    counts = {"compile-fail": sum(1 for row in manifest if row["rust_error"]),
              "native runtime check": sum(1 for row in manifest if row["category"] == "rust_runtime_check"),
              "compiled ABI": sum(1 for row in manifest if row["category"] == "runtime_abi_rejection"),
              "policy negative control": sum(1 for row in manifest if row["category"] == "runtime_policy_control")}
    print(f"PASS: {counts['compile-fail']} native Rust compile-fail studies + {counts['native runtime check']} native runtime-check study + "
          f"{counts['compiled ABI']} compiled strict-ABI study + {counts['policy negative control']} policy negative controls; {scope}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
