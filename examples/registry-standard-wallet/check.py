#!/usr/bin/env python3
"""Explicit coordinated native -> both-mode extraction -> VM/SDK example gate.

No live transactions. Requires already built shared compiler/driver tools.
Failed outputs remain under the requested fresh execution directory.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import tempfile
import tomllib

from prepare import (Commands, ENTRY, MODULE_HASH, MODULE_SHA, ROOT, TEMPLATE,
                     digest, file_identity, identity, read, require, write_json)
import cargo_bundle
from registry_codegen_evidence import encode, inspect_compiled_artifact, proper
from source_walk import source_files
from vm_failures import check_execution


def snapshot(project):
    return {"mode": "cargo_library", "package": "registry-standard-wallet", "manifest": "Cargo.toml",
            "library": "src/lib.rs", "exports": {"standard_wallet": {"entry": ENTRY, "source": "src/lib.rs"}},
            "sources": [{"path": name, "sha256": digest(read(project / name))} for name in
                        ("Cargo.toml", "Cargo.lock", "src/lib.rs", "src/generated/standard.rs")]}


PROJECT_SOURCES = ("Cargo.toml", "src/lib.rs", "examples/ledger.rs", "README.md", "dependencies.lock", "src/generated/standard.rs")


def compiled_curry(raw_module, key):
    """Frame already admitted compiled bytes without decoding as fixture data.

    All caps remain the existing package compiled-output transport defaults.
    This changes no generator, compiler, source-fixture or VM budget.
    """
    require(type(key) is bytes and len(key) == 48, "exact chosen synthetic key")
    compiled = inspect_compiled_artifact(raw_module)
    prefix = b"\xff\x02\xff\xff\x01"
    suffix = b"\xff" + encode(proper([b"\x04", (b"\x01", key), b"\x01"])) + b"\x80"
    envelope = inspect_compiled_artifact(prefix + b"\x80" + suffix)
    limits = compiled["limits"]
    # The module replaces one nil atom at root-one depth 4. Charge the whole
    # serialized copy and expanded shape before allocating the curried bytes.
    require(len(prefix) + len(raw_module) + len(suffix) <= limits["max_blob_bytes"]
            and compiled["stats"]["nodes"] + envelope["stats"]["nodes"] - 1 <= limits["max_nodes"]
            and max(compiled["stats"]["depth_root_one"] + 3, envelope["stats"]["depth_root_one"]) <= limits["max_depth_root_one"],
            "key curry exceeds compiled-output transport bounds")
    return prefix + raw_module + suffix


def project_sources(project):
    return [file_identity(project / name) for name in PROJECT_SOURCES]


def validate_project_sources(prepared, project):
    require(prepared["project_sources"] == project_sources(project),
            "materialized manifest/native tests/ledger/source bytes changed before native Cargo")


def source_window(project):
    paths = source_files(ROOT, ["crates", "experiments/backend", "examples/registry-standard-wallet", "scripts", "vendor/rue-lir"],
                         suffixes=(".rs", ".toml", ".py", ".json", ".md"))
    paths += source_files(ROOT, ["crates/chialisp-oxide-package/tests/fixtures/mainnet-standard"], suffixes=None)
    return {"repository_sources": [file_identity(ROOT / path) for path in sorted(set(paths))],
            "project_sources": project_sources(project), "project_lock": file_identity(project / "Cargo.lock"),
            "launcher": [file_identity(ROOT / name) for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml")]}


def native_observations(log):
    summaries = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", log)
    require(summaries == [("1", "0", "0", "0", "0")], "exactly one actual native test; none ignored or filtered")
    rows = [json.loads(match) for match in re.findall(r"REGISTRY_STANDARD_NATIVE (\{[^\n]+\})", log)]
    key = bytes.fromhex("9415e2b4b30977380e167096714abfdd5a594f6326dc278aa895e6bd6901953b9d8d37126e5411027eda81be8b16b97f")
    expected = {"name": "retrieved_standard_identity_program", "status": "accepted",
                "output_hex": encode(proper([proper([b"\x32", key, hashlib.sha256(b"\x01\x01").digest()])])).hex()}
    require(rows == [expected], "one actual native output witness must match independently encoded bytes")
    return rows


def self_test():
    # Constructed freshness/count controls only: no native, compiler, VM or SDK
    # execution. Coherently editing the copied policy or host harness must fail
    # even while every original template source remains unchanged.
    controls = 0
    with tempfile.TemporaryDirectory(prefix="registry-wallet-reader-") as temporary:
        project = Path(temporary)
        for name in PROJECT_SOURCES:
            path = project / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("frozen materialized bytes\n")
        prepared = {"project_sources": project_sources(project)}
        validate_project_sources(prepared, project)
        for name in ("Cargo.toml", "src/lib.rs", "examples/ledger.rs", "README.md", "dependencies.lock", "src/generated/standard.rs"):
            path = project / name
            original = read(path)
            path.write_text("coherently edited copied source: removed native assertions or ledger equality\n")
            try:
                validate_project_sources(prepared, project)
            except ValueError:
                controls += 1
            else:
                raise ValueError("escaped materialized-source freshness corruption: " + name)
            finally:
                path.write_bytes(original)
    key = bytes.fromhex("9415e2b4b30977380e167096714abfdd5a594f6326dc278aa895e6bd6901953b9d8d37126e5411027eda81be8b16b97f")
    row = {"name": "retrieved_standard_identity_program", "status": "accepted",
           "output_hex": encode(proper([proper([b"\x32", key, hashlib.sha256(b"\x01\x01").digest()])])).hex()}
    summary = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
    log = "REGISTRY_STANDARD_NATIVE " + json.dumps(row) + "\n" + summary
    require(native_observations(log) == [row], "synthetic positive native-reader control")
    for bad in (log.replace("1 passed", "0 passed"), log.replace("0 ignored", "1 ignored"), summary,
                log.replace(row["output_hex"], "80"), log + "\n" + log):
        try:
            native_observations(bad)
        except ValueError:
            controls += 1
        else:
            raise ValueError("escaped zero-test/native-witness corruption")
    key = b"\x53" * 48
    small = encode(proper([b"\x01", b"\x02"]))
    expected_curry = encode(proper([b"\x02", (b"\x01", proper([b"\x01", b"\x02"])), proper([b"\x04", (b"\x01", key), b"\x01"])]))
    require(compiled_curry(small, key) == expected_curry, "compiled raw-byte curry equals independent small tree encoding")
    # This canonical compiled-output shape deliberately exceeds source-fixture
    # nodes/depth while fitting the independently bounded iterative inspector.
    branch = b"\xff\x01" * 2500 + b"\x80"
    large = b"\xff" + branch + branch
    inspected = inspect_compiled_artifact(large)
    require(inspected["stats"]["nodes"] > 8192 and inspected["stats"]["depth_root_one"] > 128,
            "compiled/source fixture bound distinction")
    require(inspect_compiled_artifact(compiled_curry(large, key))["stats"]["nodes"] > inspected["stats"]["nodes"], "large compiled curry retained")
    for raw, kwargs in ((large, {"max_bytes": len(large) - 1}), (large, {"max_nodes": 8192}),
                        (large, {"max_depth": 128}), (large + b"\x80", {}), (b"\xff\x01", {})):
        try:
            inspect_compiled_artifact(raw, **kwargs)
        except ValueError:
            controls += 1
        else:
            raise ValueError("escaped bounded compiled-output parser control")
    print(f"PASS {controls} constructed source/native/compiled-transport controls; no execution credit")


def check(output, cli, driver, rustup):
    output = Path(output).resolve(strict=True)
    prepared = json.loads(read(output / "prepared.json"))
    require(prepared["protocol"] == "chialisp-oxide-registry-standard-wallet-prepared/v1"
            and prepared["source_root"] == str(ROOT), "actual prepared source root")
    project = output / "project"
    require(prepared["project"] == str(project), "exact prepared project")
    validate_project_sources(prepared, project)
    for row in prepared["template_sources"]:
        require(file_identity(Path(row["path"])) == row, "template changed since prepare")
    for key in ("lock", "generated_source", "fixture_provenance"):
        row = prepared[key]
        require(file_identity(Path(row["path"])) == row, "prepared input changed")
    commands = Commands(output / "execution", cli)
    commands.environment["CARGO_TARGET_DIR"] = str(project / "target")
    manifest = project / "Cargo.toml"
    toolchain = tomllib.loads(read(ROOT / "rust-toolchain.toml").decode())["toolchain"]["channel"]
    cargo = [rustup, "run", toolchain, "cargo"]
    common = ["--manifest-path", str(manifest)]
    commands.command("lock", [*cargo, "generate-lockfile", *common, "--offline"], timeout=600)
    native_before = source_window(project)
    write_json(commands.output / "native-inputs-before.json", native_before)
    commands.command("fmt", [*cargo, "fmt", *common, "--all", "--", "--check"], timeout=120)
    commands.command("native-check", [*cargo, "check", *common, "--locked"], timeout=600)
    native_log = commands.command("native-test", [*cargo, "test", *common, "--locked", "--lib", "--", "--nocapture"], timeout=600)
    native_rows = native_observations(native_log.decode())
    commands.command("native-clippy", [*cargo, "clippy", *common, "--locked", "--all-targets", "--", "-D", "warnings"], timeout=600)
    commands.command("host-ledger-build", [*cargo, "build", *common, "--locked", "--example", "ledger"], timeout=600)
    require(source_window(project) == native_before, "native library/test/host-runner build inputs changed during native Cargo phases")
    # Finish every host Cargo phase before the frozen driver extraction window.
    checked = commands.registry("generated-check", "generate-rust", "--registry", output / "registry",
        "--lock", project / "dependencies.lock", "--name", "standard", "--output", project / "src/generated/standard.rs", "--check")
    require(checked["status"] == "checked", "exact offline source regeneration")
    compiler = commands.command("rustc-query", [rustup, "run", toolchain, "rustc", "-vV"]).decode()
    declaration = commands.command("driver-query", [driver, "--compiler-version"]).decode()
    require(declaration == compiler.splitlines()[0] + "\n", "independently queried driver/compiler declaration")
    driver_sha = file_identity(driver, 512 * 1024 * 1024)["sha256"]

    def quiet():
        return {"source_window": source_window(project), "consumer": snapshot(project),
                "binaries": [file_identity(path, 512 * 1024 * 1024) for path in (cli, driver, project / "target/debug/examples/ledger")],
                "launcher": [file_identity(ROOT / name) for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml")]}

    before = quiet()
    require(before["source_window"] == native_before, "LAST-host source window differs from tested native sources")
    write_json(commands.output / "quiet-before.json", before)
    commands.command("subset", [cli, "check", *common, "--locked"], timeout=600)
    exports, runs = {}, []
    for mode in ("off", "on"):
        bundle = commands.output / ("bundle-" + mode)
        argv = [cli, "build", *common, "--locked", "--output-dir", bundle]
        if mode == "off": argv.append("--no-optimize")
        commands.command("extract-" + mode, argv, timeout=600)
        authoring = snapshot(project)
        export = cargo_bundle.load_declared_export(bundle, root=project, authoring=authoring, selected_entry=ENTRY,
            compiler=compiler, driver_sha256=driver_sha, toolchain=toolchain,
            current_authoring=lambda: snapshot(project), optimize=mode == "on", cargo_options=[*common, "--locked"])
        require(export.manifest["schema_version"] == 2 and len(export.manifest["entries"]) == 1,
                "complete single-export schema2 bundle")
        sources = {source.label: source for source in export.source_files}
        expected_sources = {"package/" + name for name in ("Cargo.toml", "Cargo.lock", "src/lib.rs", "src/generated/standard.rs")}
        require(set(sources) == expected_sources, "complete automatic selected-library source receipt")
        for label, source in sources.items():
            require(source.contents == read(project / label.removeprefix("package/")), "receipt must bind actual authoring source")
            if label.endswith(".rs"):
                require(source.compiler_read_count >= 1 and source.kinds == ("compiler_text",), "actual compiler-read generated/author source")
        require(export.source_receipt["unit"]["features"] == [], "native vm feature must remain absent during extraction")
        selected = export.manifest["entries"][0]
        artifact = selected["artifact"]
        require(artifact["backend"] == "rust_mir" and artifact["optimize_lir"] == (mode == "on"), "requested Rust mode")
        compiled = bytes.fromhex(artifact["clvm_hex"])
        inspected = inspect_compiled_artifact(compiled)
        require(inspected["identity"]["module_tree_hash"] == artifact["tree_hash"] != MODULE_HASH, "wrapper/canonical identities remain distinct")
        exports[mode] = {"manifest": str(export.manifest_path), "module": inspected["identity"], "compiled_transport": inspected, "source_receipt": export.source_receipt}
        runs.append({"mode": mode, "module_hex": compiled.hex()})
    canonical = bytes.fromhex(json.loads(read(ROOT / "crates/chialisp-oxide-package/tests/fixtures/mainnet-standard/expected.json"))["module_hex"])
    require(digest(canonical) == MODULE_SHA and identity(canonical)["module_tree_hash"] == MODULE_HASH, "independent exact canonical reference")
    request = commands.output / "ledger-request.json"
    write_json(request, {"protocol": "chialisp-oxide-registry-standard-wallet-request/v1", "canonical_module_hex": canonical.hex(), "runs": runs})
    # No Cargo phase follows extraction. Execute only the already built harness.
    runner = project / "target/debug/examples/ledger"
    result = commands.command("ledger", [runner, request], timeout=120, json_result=True)
    require(result["protocol"] == "chialisp-oxide-registry-standard-wallet-ledger/v1" and result["status"] == "executed"
            and result["sdk_transaction_attempts"] == 18 and result["accepted"] == 3
            and result["typed_consensus_rejected"] == 12 and result["runtime_umbrellas"] == 3, "scoped actual SDK observations")
    require([row["mode"] for row in result["builds"]] == ["off", "on", "canonical_reference"], "both Rust modes and separately executed reference")
    outputs = []
    for build in result["builds"]:
        outputs.append(check_execution(build["primary_vm"]))
        require(check_execution(build["malformed_primary_vm"])["status"] == "rejected"
                and build["malformed_primary_vm"]["failure"] == {"kind": "program", "code": "path_into_atom"}, "typed malformed VM witness")
        require(build["module_tree_hash"] == (MODULE_HASH if build["mode"] == "canonical_reference" else exports[build["mode"]]["module"]["module_tree_hash"]), "SDK uses exact selected module")
        attempts = build["attempts"]
        require(len(attempts) == 6 and build["sdk_transaction_attempts"] == 6, "six actual submissions per selected reveal")
        require([row["name"] for row in attempts] == ["empty_signature", "wrong_key_signature", "wrong_message_signature", "malformed_solution", "signed_complete_spend", "consumed_input_replay"], "complete named attempt sequence")
        key = bytes.fromhex(build["committed_synthetic_key"])
        require(len(key) == 48, "chosen synthetic key width")
        raw_module = canonical if build["mode"] == "canonical_reference" else bytes.fromhex(next(row["module_hex"] for row in runs if row["mode"] == build["mode"]))
        reveal = compiled_curry(raw_module, key)
        require(inspect_compiled_artifact(reveal)["identity"]["module_tree_hash"] == build["instance_tree_hash"], "independent full key curry bytes/hash")
        for index, attempt in enumerate(attempts):
            transaction = attempt["transaction"]
            require(len(transaction["spends"]) == 1 and len(bytes.fromhex(transaction["signature_hex"])) == 96, "one actual spend and bounded signature")
            spend = transaction["spends"][0]
            require(spend["reveal_hex"] == reveal.hex() and spend["coin"]["puzzle_hash"] == build["instance_tree_hash"]
                    and spend["coin"]["amount"] == 1000 and spend["solution_hex"] == ("80" if index == 3 else build["solution_hex"]),
                    "actual submission must use own committed instance and exact witness")
        require(attempts[0]["transaction"]["signature_hex"] == "c0" + "00" * 95
                and all(attempts[index]["transaction"]["signature_hex"] != "c0" + "00" * 95 for index in (1, 2, 4)),
                "empty and actually nonempty wrong-key/wrong-message/signed submissions")
        require(attempts[4]["transaction"] == attempts[5]["transaction"], "replay the exact accepted bundle")
        for attempt in attempts[:3]:
            require(attempt["status"] == "consensus_rejected" and attempt["code"] == "BadAggregateSignature"
                    and attempt["full_simulator_equality"] is True, "typed signature failure/full in-memory equality")
        require(attempts[3]["status"] == "runtime_umbrella" and attempts[3]["semantic_rejection_credit"] is False
                and attempts[3]["code"] == "GeneratorRuntimeError" and attempts[3]["full_simulator_equality"] is True, "runtime umbrella is not semantic rejection")
        child = attempts[4]["child"]
        require(attempts[4]["status"] == "accepted" and child["amount"] == 1000 and child["puzzle_hash"] == "42" * 32
                and child["parent_coin_id"] == attempts[4]["input_coin_id"], "exact accepted child witness")
        require(attempts[5]["status"] == "consensus_rejected" and attempts[5]["code"] == "DoubleSpend"
                and attempts[5]["full_simulator_equality"] is True, "exact replay rejection")
    require(outputs[0] == outputs[1] == outputs[2] and outputs[0]["status"] == "accepted", "exact cross-mode and canonical conditions")
    require(quiet() == before, "source/compiler inputs changed during coordinated extraction/execution")
    write_json(commands.output / "report.json", {"protocol": "chialisp-oxide-registry-standard-wallet-evidence/v1", "status": "executed",
        "prepared": prepared, "compiler": compiler, "driver_sha256": driver_sha, "exports": exports,
        "native_test_groups": len(native_rows), "native_output_observations": len(native_rows), "native": native_rows,
        "native_inputs": native_before, "last_host_inputs": before, "primary_vm_observations": 6,
        "ledger": result, "commands": commands.commands,
        "scope": "Finite executed native, primary VM and complete SDK spend observations over retrieved bytes; exact source and artifact consistency is not source authenticity, binary authentication, a general typed binding, compiler preservation or production readiness."})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--registry-cli", type=Path)
    parser.add_argument("--driver", type=Path)
    parser.add_argument("--rustup", type=Path, default=Path(shutil.which("rustup") or "/nonexistent/rustup"))
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    require(args.execute or args.self_test, "Choose --self-test or coordinated --execute")
    if args.self_test:
        self_test()
    if args.execute:
        require(args.output is not None and args.registry_cli is not None and args.driver is not None,
                "--execute requires --output, --registry-cli and --driver")
        check(args.output, args.registry_cli.resolve(strict=True), args.driver.resolve(strict=True), args.rustup.resolve(strict=True))
