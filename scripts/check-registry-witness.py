#!/usr/bin/env python3
"""Explicit production witness authoring and finite source-mutant evidence.

Finish shared builds and source formatting before --execute. This gate uses
prebuilt tools, never rebuilds the shared driver, and preserves partial reports.
Native Cargo/reference compilation precedes each actual compiler query/window.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tomllib

import cargo_bundle
from cargo_source_receipt import decode_json
from registry_codegen_evidence import (
    FrozenFiles, Inconclusive, decode, digest, encode, facade_feature_record,
    file_identity, identity, inspect_compiled_artifact, proper, read, require,
    unhex, write_json,
)
from registry_witness_evidence import (
    ALIASES, CRATE, EXPORTS, HELPER, HELPER_SHA256, LIMITS, MUTANTS, PACKAGE,
    check_command_logs, check_generated, check_resolve, inspect_ir, mutant_expected, mutate,
)
from source_walk import source_files
from vm_failures import PROTOCOL

ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / "scripts/registry_witness_fixture"
spec = importlib.util.spec_from_file_location("registry_witness_process_gate", ROOT / "scripts/check-registry-codegen.py")
process_gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(process_gate)


def fixtures():
    modules = {"multiply": bytes.fromhex("ff12ff02ff0580")}
    modules.update({f"quote{size}": encode((b"\x01", b"\xa5" * size)) for size in (257, 8192)})
    cases = []

    def integer(value):
        return b"" if value == 0 else value.to_bytes((value.bit_length() + 8) // 8, "big", signed=True)

    for label, value, tail, answer in (
        ("zero", b"", b"", 0), ("positive", b"\x15", b"", 42),
        ("negative", b"\xff", b"", -2), ("signed-boundary", b"\x00\x80", b"", 256),
        ("unbounded-clvm", integer(1 << 128), b"", 1 << 129),
        ("padded-zero", b"\x00\x00", b"", 0), ("padded-positive", b"\x00\x15", b"", 42),
        ("extra-field", b"\x15", proper([b"\x7f"]), 42), ("improper-tail", b"\x15", b"\xff\x80", 42)):
        solution = (value, tail)
        cases.append({"label": label, "solution": solution, "environment": (b"\x02", solution),
                      "expected": {"status": "accepted", "output_hex": encode(integer(answer)).hex()}})
    for label, solution, code in (("missing", b"", "path_into_atom"), ("scalar", b"\x01", "path_into_atom"),
                                  ("pair-is-not-integer", ((b"\x01", b""), b""), "invalid_op_arg")):
        cases.append({"label": label, "solution": solution, "environment": (b"\x02", solution),
                      "expected": {"status": "rejected", "code": code}})
    vectors = {"multiply": cases}
    for size in (257, 8192):
        vectors[f"quote{size}"] = [{"label": label, "solution": environment, "environment": environment,
            "expected": {"status": "accepted", "output_hex": encode(b"\xa5" * size).hex()}}
            for label, environment in (("nil", b""), ("nonutf8-atom", b"\xff\x80"),
                ("improper-pair", (b"\x00\x01", b"\xff")),
                ("nested-padded", proper([b"\x00\x01", proper([b"\xff\x80"])]))) ]
    return modules, vectors


class WitnessRun(process_gate.Run):
    def __init__(self, args, *, mutant=None, origin=None, sdk=False):
        super().__init__(args)
        self.mutant, self.origin, self.sdk = mutant, origin, sdk
        self.aliases = ALIASES if origin is None else ("multiply",)
        self.exports = EXPORTS if origin is None else EXPORTS[:2]
        self.host_inputs = FrozenFiles()
        self.origin_inputs = FrozenFiles()
        self.native_inputs = FrozenFiles()
        self.toolchain = tomllib.loads(read(ROOT / "rust-toolchain.toml").decode())["toolchain"]["channel"]

    def materialize(self):
        files = source_files(ROOT, [TEMPLATE.relative_to(ROOT).as_posix()], suffixes=None)
        expected = ["Cargo.toml", "README.md", "src/lib.rs", "src/mutant.rs", "examples/compile-reference.rs"]
        require(files == sorted((TEMPLATE / name).relative_to(ROOT).as_posix() for name in expected),
                "closed ordinary witness fixture inventory required")
        self.origin_inputs.observe([ROOT / path for path in files])
        self.project.mkdir()
        (self.project / "src/generated").mkdir(parents=True)
        (self.project / "examples").mkdir()
        for source, target in (("src/mutant.rs" if self.origin else "src/lib.rs", "src/lib.rs"),
                               ("README.md", "README.md"), ("examples/compile-reference.rs", "examples/compile-reference.rs")):
            (self.project / target).write_bytes(read(TEMPLATE / source))
        manifest = read(TEMPLATE / "Cargo.toml").decode()
        require(tomllib.loads(manifest)["package"]["name"] == PACKAGE
                and tomllib.loads(manifest)["workspace"] == {}, "ordinary isolated Cargo manifest required")
        facade = self.copy_sdk() if self.sdk else ROOT / "crates/chialisp-oxide"
        rewrites = {"@FACADE_PATH@": facade, "@REFERENCE_PATH@": ROOT / "crates/chialisp-oxide-reference"}
        for marker, path in rewrites.items():
            require(manifest.count('"' + marker + '"') == (2 if marker == "@FACADE_PATH@" else 1), "closed path rewrite required")
            manifest = manifest.replace('"' + marker + '"', json.dumps(str(path)))
        (self.project / "Cargo.toml").write_text(manifest, encoding="utf-8")
        write_json(self.output / "materialization.json", {"template": [file_identity(ROOT / path) for path in files],
            "rewrites": {marker: str(path) for marker, path in rewrites.items()},
            "materialized_manifest": file_identity(self.project / "Cargo.toml")})
        if self.origin is not None:
            paths = source_files(self.origin.output, ["registry"], suffixes=None)
            paths += ["consumer/dependencies.lock", "consumer/src/generated/multiply.rs"]
            self.origin_inputs.observe([self.origin.output / path for path in paths])
            self.origin_inputs.observe([self.origin.output / "report.json", self.origin.output / "quiet-before.json",
                self.origin.output / "quiet-after.json", self.origin.output / "retained-before.json", self.origin.output / "retained-after.json"])
            # This store was created here by actual registry publication. Copy
            # its complete bounded inventory, including protocol .publish.lock;
            # the empty file is not evidence of an active process lock.
            for relative in paths:
                path = self.origin.output / relative
                require(path.stat().st_size <= 1024 * 1024, "copied exact host input bound")
                target = self.output / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(read(path, 1024 * 1024))
            require(read(self.project / "src/generated/multiply.rs") == read(self.origin.project / "src/generated/multiply.rs"),
                    "actual emitted source copy differs")
            self.source_mutation()

    def copy_sdk(self):
        source, target = ROOT / "crates/chialisp-oxide", self.output / "copied-sdk"
        files = source_files(ROOT, [source.relative_to(ROOT).as_posix()], suffixes=None)
        require(len(files) <= 128 and sum((ROOT / path).stat().st_size for path in files) <= 8 * 1024 * 1024,
                "copied SDK source bounds")
        self.origin_inputs.observe([ROOT / path for path in files])
        for relative in files:
            destination = target / (ROOT / relative).relative_to(source)
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(read(ROOT / relative))
        path = target / "Cargo.toml"
        raw = read(path).decode()
        for crate in ("chialisp-oxide-macros", "chialisp-oxide-vm"):
            old = '"../' + crate + '"'
            require(raw.count(old) == 1, "closed copied SDK manifest rewrite required")
            raw = raw.replace(old, json.dumps(str(ROOT / "crates" / crate)))
        path.write_text(raw, encoding="utf-8")
        body = target / "src/apply.rs"
        old = b"pub fn apply<'a>(_program: &Tree<'a>, _environment: &Tree<'a>) -> Tree<'static> {\n    // A bare panic lowers to one `panic` lang-item call that the compiler\n    // bridge fingerprints; a message would pull in `fmt::Arguments`.\n    panic!()\n}"
        original = read(body)
        require(original.count(old) == 1, "unique actual reserved SDK Apply source mutation site required")
        changed = original.replace(old, old.replace(b"panic!()\n}", b"Tree::Atom(&[])\n}"), 1)
        body.write_bytes(changed)
        self.sdk_mutation = {"origin": file_identity(source / "src/apply.rs"), "mutant": file_identity(body),
            "recipe": "copied vm-less reserved Apply body returns nil instead of bare panic; vm dev body unchanged",
            "manifest": file_identity(path), "scope": "Source recipe only until actual native and subset commands complete."}
        return target

    def source_mutation(self):
        path = self.project / "src/generated/multiply.rs"
        arguments = ["generate-witness-rust", "--registry", self.output / "registry", "--lock", self.project / "dependencies.lock",
                     "--name", "multiply", "--output", path, "--check"]
        original = read(path, 8192)
        checked = self.registry("copied-original-regeneration-check", *arguments)
        check_generated(checked, original, "multiply", self.origin.modules["multiply"],
                        read(self.project / "dependencies.lock"), self.origin.records["multiply"]["generation"]["object_id"], "checked")
        if self.sdk:
            self.mutation_record = self.sdk_mutation
            return
        changed = mutate(original, self.mutant)
        (self.output / "original-generated-multiply.rs").write_bytes(original)
        path.write_bytes(changed)
        failed = self.registry("source-mutant-regeneration-conflict", *arguments, expected=1)
        require(failed["status"] == "error" and failed["kind"] == "conflict" and read(path) == changed,
                "actual edited source must fail read-only regeneration without overwrite")
        self.mutation_record = {"name": self.mutant, "original": file_identity(self.output / "original-generated-multiply.rs"),
            "mutant": file_identity(path), "regeneration_response": failed,
            "scope": "Regeneration conflict only; native, subset and paired VM controls recorded separately."}

    def resolve_and_generate(self, modules):
        self.modules = modules
        summaries = {alias: self.publish_plain(alias, raw) for alias, raw in modules.items()}
        lock, store = self.project / "dependencies.lock", self.output / "registry"
        lock_bytes = read(lock, 1024 * 1024)
        wire = decode_json(lock_bytes)
        require(set(wire) == {"schema_version", "protocol", "scope", "dependencies"}
                and wire["schema_version"] == 1 and wire["protocol"] == "chialisp-oxide-dependencies/v1"
                and wire["scope"] == "local_exact_artifact_pins"
                and [row["name"] for row in wire["dependencies"]] == list(ALIASES)
                and all(row["binding"] is None for row in wire["dependencies"]), "complete exact opaque lock required")
        full = self.registry("resolve-complete-lock", "resolve", "--lock", lock, "--registry", store)
        check_resolve(full, lock_bytes, wire["dependencies"])
        records = {}
        for pin in wire["dependencies"]:
            alias = pin["name"]
            selected = self.registry("resolve-selected-" + alias, "resolve", "--lock", lock, "--registry", store, "--name", alias)
            check_resolve(selected, lock_bytes, [pin])
            looked = self.registry("lookup-" + alias, "lookup", "--registry", store, "--module-hash", pin["artifact"]["module_tree_hash"])
            matches = [row for row in looked["packages"] if row["object_id"] == pin["object_id"]]
            got = self.registry("get-" + alias, "get", "--registry", store, "--object-id", pin["object_id"])
            package = store / "objects" / (pin["object_id"] + ".json")
            verified = self.registry("verify-" + alias, "verify", "--package", package)
            require(looked["status"] == "lookup_complete" and len(matches) == 1
                    and got["status"] == verified["status"] == "verified"
                    and got["package"] == verified["package"] == matches[0] == summaries[alias]
                    and digest(read(package)) == pin["object_id"], "actual selected envelope retrieval differs")
            decoded = decode_json(read(package))
            require(unhex(decoded["artifact"]["clvm_hex"]) == modules[alias]
                    and decoded["artifact"]["identity"] == pin["artifact"] == identity(modules[alias]),
                    "exact selected raw program/full package pin differs")
            source = self.project / "src/generated" / (alias + ".rs")
            arguments = ["generate-witness-rust", "--registry", store, "--lock", lock, "--name", alias, "--output", source]
            first = self.registry("generate-witness-" + alias, *arguments)
            raw = read(source, LIMITS["max_source_bytes"])
            record = check_generated(first, raw, alias, modules[alias], lock_bytes, pin["object_id"], "created")
            for suffix, status, extra in (("repeat", "already_present", []), ("check", "checked", ["--check"])):
                result = self.registry(f"generate-witness-{alias}-{suffix}", *arguments, *extra)
                require(check_generated(result, read(source), alias, modules[alias], lock_bytes, pin["object_id"], status) == record
                        and read(source) == raw, "repeat/check changed exact generated source/record")
            records[alias] = {"generation": record, "source": file_identity(source), "resolve": selected,
                              "package": file_identity(package), "summary": verified["package"]}
        self.records = records
        controls = self.host_controls(wire)
        write_json(self.output / "resolution-generation.json", {"resolve": full, "aliases": records, "host_controls": controls})
        self.freeze_host()
        return {"complete_resolve": full, "aliases": records, "host_controls": controls}

    def host_controls(self, wire):
        changed = copy.deepcopy(wire)
        changed["dependencies"][0]["artifact"]["blob_sha256"] = "00" * 32
        path = self.output / "unselected-corrupt-pin.lock"
        write_json(path, changed)
        failed = self.registry("generation-unselected-corrupt-pin", "generate-witness-rust", "--lock", path,
            "--registry", self.output / "registry", "--name", "quote8192", "--output", self.output / "must-not-exist.rs", expected=1)
        require(failed["status"] == "error" and failed["kind"] == "digest_mismatch"
                and not (self.output / "must-not-exist.rs").exists(), "generation skipped broken unselected pin")
        control = self.output / "unrelated-corrupt-object.control.json"
        control.write_bytes(b"{}")
        corrupt = self.output / "registry/objects" / (digest(b"{}") + ".json")
        corrupt.write_bytes(read(control))
        before = file_identity(corrupt)
        try:
            broken = self.registry("generation-unrelated-corrupt-object", "generate-witness-rust", "--lock", self.project / "dependencies.lock",
                "--registry", self.output / "registry", "--name", "multiply", "--output", self.output / "must-not-exist.rs", expected=1)
            require(broken["status"] == "error" and broken["kind"] == "corrupt_object"
                    and not (self.output / "must-not-exist.rs").exists(), "generation skipped unrelated bad store object")
        finally:
            require(file_identity(corrupt) == before, "corrupt-object control changed during command")
            corrupt.unlink()
        return {"unselected_pin": {"input": file_identity(path), "response": failed},
            "unrelated_object": {"retained": file_identity(control), "actual_store_input": before, "response": broken},
            "scope": "Actual offline host rejection controls only; zero VM/policy rejection credit."}

    def freeze_host(self):
        paths = source_files(self.output, ["registry"], suffixes=None)
        paths += ["consumer/dependencies.lock"]
        for name in ("resolution-generation.json", "unselected-corrupt-pin.lock", "unrelated-corrupt-object.control.json"):
            if (self.output / name).exists():
                paths.append(name)
        paths += [alias + ".package.json" for alias in ALIASES if (self.output / (alias + ".package.json")).exists()]
        self.host_inputs.observe([self.output / path for path in sorted(set(paths))])

    def snapshot(self):
        paths = source_files(self.project, ["src", "examples"], suffixes=None)
        paths += ["Cargo.toml", "Cargo.lock", "dependencies.lock", "README.md"]
        return {"mode": "cargo_library", "package": PACKAGE, "manifest": "Cargo.toml", "library": "src/lib.rs",
            "exports": {name: {"entry": CRATE + "::" + name, "source": "src/lib.rs"} for name in self.exports},
            "sources": [{"path": path, "sha256": digest(read(self.project / path))} for path in sorted(paths)]}

    def quiet_inputs(self):
        result = super().quiet_inputs()
        paths = source_files(ROOT, [TEMPLATE.relative_to(ROOT).as_posix()], suffixes=None)
        paths += ["scripts/check-registry-witness.py", "scripts/registry_witness_evidence.py",
                  "scripts/test_registry_witness_evidence.py", "puzzles/sha256tree/puzzle.rs", "puzzles/sha256tree/puzzle.json",
                  "puzzles/inner-puzzle/puzzle.rs", "docs/dynamic-apply.md"]
        provenance = self.helper_provenance()
        paths += [Path(row["path"]).relative_to(ROOT).as_posix() for row in provenance["retained_inputs"]]
        result["witness_authoring_sources"] = [file_identity(ROOT / path) for path in sorted(set(paths))]
        result["native_launcher"] = file_identity(self.args.rustup, 512 * 1024 * 1024)
        self.freeze_host()
        result["exact_lock_store_inputs"] = self.host_inputs.files()
        if self.sdk:
            result["copied_sdk"] = [file_identity(self.output / path) for path in source_files(self.output, ["copied-sdk"], suffixes=None)]
        return result

    def helper_provenance(self):
        value = decode_json(read(ROOT / "puzzles/sha256tree/puzzle.json"))["provenance"]
        upstream, span, license = value["upstream"], value["source_span"], value["license"]
        require(upstream["repository"] == "https://github.com/Chia-Network/chia_puzzles"
                and upstream["revision"] == "cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8"
                and span["start_line"] == 4 and span["end_line"] == 10
                and span["sha256"] == "946a967edff3fdc1eed3260cc7a1f28972e6d78c3ea248a814a0cfe2599dec7e"
                and license["spdx"] == "Apache-2.0"
                and license["sha256"] == "7db0cf1060026bf2a544ea29cf9a96fba7d3781f93a1d06d6fee18251f1c207a",
                "fixed source-span/license provenance changed")
        retained = []
        for record, key in ((upstream, "source_sha256"), (span, "sha256"), (license, "sha256")):
            path = ROOT / record["snapshot"]
            require(path.resolve().is_relative_to(ROOT) and digest(read(path)) == record[key], "pinned helper provenance bytes differ")
            retained.append(file_identity(path))
        require(read(ROOT / span["snapshot"]) == b"".join(read(ROOT / upstream["snapshot"]).splitlines(keepends=True)[3:10]),
                "retained helper span differs from exact upstream lines")
        return {"retained_inputs": retained, "repository": upstream["repository"], "revision": upstream["revision"],
                "source_span": {"start_line": 4, "end_line": 10}, "license": license["spdx"],
                "scope": "Pinned source/algorithm/license identities only; not authorship, cryptography or compiler preservation."}

    def native_phase(self):
        cargo = [self.args.rustup, "run", self.toolchain, "cargo"]
        manifest = self.project / "Cargo.toml"
        original = {alias: read(self.project / "src/generated" / (alias + ".rs")) for alias in self.aliases}
        self.command("ordinary-cargo-fmt", [*cargo, "fmt", "--manifest-path", manifest])
        for alias, raw in original.items():
            path = self.project / "src/generated" / (alias + ".rs")
            require(read(path) == raw, "ordinary cargo fmt changed exact generated/mutant source")
            result = self.registry("post-format-generation-check-" + alias, "generate-witness-rust", "--registry", self.output / "registry",
                "--lock", self.project / "dependencies.lock", "--name", alias, "--output", path, "--check", expected=1 if self.mutant else 0)
            if self.mutant:
                require(result["status"] == "error" and result["kind"] == "conflict", "source mutant must remain a regeneration conflict")
            else:
                require(result["status"] == "checked" and result["generation"]["source_sha256"] == digest(raw), "post-format generation identity differs")
        self.command("native-lock", [*cargo, "generate-lockfile", "--manifest-path", manifest, "--offline"])
        native_paths = [self.project / path for path in source_files(self.project, ["src", "examples"], suffixes=None)]
        native_paths += [self.project / name for name in ("Cargo.toml", "Cargo.lock", "dependencies.lock", "README.md")]
        if self.sdk:
            native_paths += [self.output / path for path in source_files(self.output, ["copied-sdk"], suffixes=None)]
        self.native_inputs.observe(native_paths)
        write_json(self.output / "native-inputs-before.json", self.native_inputs.files())
        # Native Rust and the reference example read real path dependencies,
        # not merely the consumer. Bind the same complete implementation/tool
        # closure before their compilation and across the later extraction.
        self.native_before = self.quiet_inputs()
        write_json(self.output / "native-quiet-before.json", self.native_before)
        options = ["--manifest-path", manifest, "--locked", "--offline", "--target-dir", self.output / "native-target"]
        self.command("native-check", [*cargo, "check", "--lib", *options], timeout=600)
        result = {"native_valid": True, "scope": "Actual ordinary Cargo check only; policy correctness is separate."}
        if self.origin is None:
            tests = self.command("native-tests", [*cargo, "test", "--lib", *options], timeout=600)
            require(b"test result: ok. 4 passed; 0 failed; 0 ignored;" in tests, "four actual native groups required")
            compiled = self.command("independent-chialisp-compile", [*cargo, "run", "--example", "compile-reference", *options], timeout=600)
            require(compiled == "".join(alias + ":" + module.hex() + "\n" for alias, module in self.modules.items()).encode(),
                    "independent actual Chialisp module bytes differ")
            helper_source = read(ROOT / "puzzles/sha256tree/puzzle.rs")
            require(helper_source.count(HELPER) == 1 and digest(HELPER) == HELPER_SHA256,
                    "local generated hash helper must match the actual reviewed puzzle body")
            result.update(native_test_groups=4, reference={alias: identity(raw) for alias, raw in self.modules.items()},
                helper={"sha256": HELPER_SHA256, "bytes": len(HELPER), "local_source": file_identity(ROOT / "puzzles/sha256tree/puzzle.rs"),
                        "provenance": self.helper_provenance(),
                        "existing_precedent": file_identity(ROOT / "puzzles/inner-puzzle/puzzle.rs")})
        after = self.quiet_inputs()
        write_json(self.output / "native-quiet-after.json", after)
        require(after == self.native_before, "complete native compiler/reference source/tool input window changed")
        self.freeze_host()
        return result

    def extract_sdk_rejection(self):
        self.driver_compiler_declaration = self.command("actual-driver-compiler-query", [self.args.driver, "--compiler-version"]).decode()
        self.compiler = self.command("actual-pinned-rustc-query", [self.args.rustup, "run", self.toolchain, "rustc", "-vV"]).decode()
        self.compiler_fields = process_gate.check_compiler_queries(self.driver_compiler_declaration, self.compiler)
        self.before = self.quiet_inputs()
        write_json(self.output / "quiet-before.json", self.before)
        self.command("subset-sdk-body-rejection", [self.args.registry_cli, "check", "--manifest-path", self.project / "Cargo.toml", "--locked"],
                     expected=1, timeout=600)
        diagnostic = read(self.output / "subset-sdk-body-rejection.stderr")
        require(b"reviewed native apply primitive rejected" in diagnostic,
                "modified SDK body must fail the actual reserved-body check, not an unrelated error")
        return {"mutation": self.sdk_mutation, "native_valid": True, "subset_rejected": True,
            "diagnostic": file_identity(self.output / "subset-sdk-body-rejection.stderr"),
            "scope": "Actual native-valid copied SDK body rejected by the subset checker; no compiled mutant or VM credit."}

    def inspect(self):
        snapshot, results, artifacts = self.snapshot(), {}, {}
        for mode in ("off", "on"):
            lane = {}
            for name in self.exports:
                export = cargo_bundle.load_declared_export(self.output / ("bundle-" + mode), root=self.project,
                    authoring=snapshot, selected_entry=CRATE + "::" + name, compiler=self.compiler,
                    driver_sha256=next(row["sha256"] for row in self.before["binaries"] if row["path"] == str(self.args.driver)),
                    toolchain=self.toolchain, current_authoring=self.snapshot, optimize=mode == "on",
                    cargo_options=["--manifest-path", str(self.project / "Cargo.toml"), "--locked"])
                require(export.manifest["schema_version"] == 2 and export.program["ir_version"] == 4
                        and export.program["environment"] == "raw", "actual raw v4 schema2 export required")
                expected = {"package/Cargo.toml", "package/Cargo.lock", "package/src/lib.rs"}
                expected |= {"package/src/generated/" + alias + ".rs" for alias in self.aliases}
                require({source.label for source in export.source_files} == expected, "complete actual generated-library source inventory differs")
                for source in export.source_files:
                    require(source.contents == read(self.project / source.label.removeprefix("package/")), "actual compiler-read source differs")
                    if source.label.endswith(".rs"):
                        require(source.kinds == ("compiler_text",) and source.compiler_read_count >= 1, "compiler-read local helper/source required")
                require(export.source_receipt["unit"] == {"crate_name": CRATE, "analysis_target": export.manifest["analysis_target"],
                    "features": [], "library_source": "package/src/lib.rs"}, "actual selected-unit features/library differ")
                require(export.source_receipt["snapshots"] == {"package_manifest": "package/Cargo.toml", "workspace_manifest": "package/Cargo.toml",
                    "lockfile": "package/Cargo.lock", "build_script": None}, "named actual launcher snapshots differ")
                constants = inspect_ir(export.program, baseline=self.mutant is None)
                row = next(row for row in export.manifest["entries"] if row["entry"] == CRATE + "::" + name)
                raw = unhex(row["artifact"]["clvm_hex"])
                compiled = inspect_compiled_artifact(raw)
                require(b"\xa5" * 257 not in raw and b"\xa5" * 8192 not in raw, "large witness payload embedded in compiled Rust artifact")
                artifact = export.manifest_path.parent / row["clvm"]
                require(read(artifact) == (raw.hex() + "\n").encode()
                        and compiled["identity"]["module_tree_hash"] == row["artifact"]["tree_hash"], "exact compiled artifact differs")
                published = self.registry(f"admit-{mode}-{name}", "publish", "--bundle", export.manifest_path,
                    "--entry", CRATE + "::" + name, "--registry", self.output / "compiled-registry")
                summary = published["package"]
                require(summary["kind"] == "rust_cargo_export" and summary["source_scope"] == "declared_files_only"
                        and summary["source_files"] == len(expected) and summary["blob_sha256"] == compiled["identity"]["blob_sha256"]
                        and summary["module_tree_hash"] == compiled["identity"]["module_tree_hash"], "actual complete compiled package admission differs")
                package = self.output / "compiled-registry/objects" / (summary["object_id"] + ".json")
                verified = self.registry(f"verify-{mode}-{name}", "verify", "--package", package)
                require(verified["package"] == summary and digest(read(package)) == summary["object_id"], "exact admitted package reread differs")
                lane[name] = {"manifest": file_identity(export.manifest_path), "ir": file_identity(export.path),
                    "artifact": {**compiled["identity"], "file": file_identity(artifact)}, "compiled_output_inspection": compiled,
                    "ir_constants": constants, "source_receipt": export.source_receipt, "package": summary,
                    "sources": [{"label": source.label, "sha256": source.sha256, "bytes": len(source.contents),
                        "kinds": list(source.kinds), "compiler_read_count": source.compiler_read_count} for source in export.source_files]}
                artifacts[(mode, name)] = artifact
            directory = self.output / "compiler-target/chialisp-oxide/cargo" / export.manifest["driver_sha256"] / export.manifest["analysis_target"] / "debug/.fingerprint"
            paths = sorted(directory.glob("chialisp-oxide-*/lib-chialisp_oxide.json"))
            require(len(paths) == 1, "one actual vm-less facade feature record required")
            feature = facade_feature_record(paths[0])
            require(self.feature_inputs.observe(paths) == [feature["file"]], "actual facade feature record changed during retention")
            self.feature_records[mode] = feature
            results[mode] = lane
        require(all(results["off"][name]["ir"]["sha256"] == results["on"][name]["ir"]["sha256"] for name in self.exports),
                "same actual Rust source must retain the same paired IR")
        return results, artifacts

    def replay(self, modules, vectors, artifacts):
        canonical = {}
        for alias, cases in vectors.items():
            path = self.output / (alias + ".canonical.hex")
            path.write_text(modules[alias].hex() + "\n", encoding="ascii")
            actual = self.batch("canonical-original-" + alias, path, cases, framed=True)
            require(actual == [case["expected"] for case in cases], "actual canonical result/full output or typed code differs from independent expectation")
            canonical[alias] = actual
        curry = encode(proper([b"\x02", (b"\x01", decode(modules["multiply"])),
                              proper([b"\x04", (b"\x01", b"\x02"), b"\x01"])]))
        path = self.output / "multiply.curried.hex"
        path.write_text(curry.hex() + "\n", encoding="ascii")
        require(self.batch("canonical-curried-multiply", path, vectors["multiply"]) == canonical["multiply"],
                "actual independently curried comparator differs")
        path = self.output / "wrong-path6.hex"
        path.write_text("06\n", encoding="ascii")
        environment = vectors["multiply"][1]["environment"]
        require(self.batch("wrong-path6-direct", path, [{"label": "actual-path6-before-guard-discriminator", "solution": environment}])
                == [{"status": "rejected", "code": "path_into_atom"}], "actual wrong path6 must distinguish guard order")
        for alias, cases in vectors.items():
            program = decode(modules[alias])
            matching = [{"label": case["label"], "solution": (program, case["environment"])} for case in cases]
            if alias == "multiply":
                changed = decode(bytes.fromhex("ff10ff02ff0580"))
            else:
                size = int(alias.removeprefix("quote"))
                changed = (b"\x01", b"\xa5" * (size - 1) + b"\xa4")
            other = "quote257" if alias != "quote257" else "quote8192"
            wrong_programs = [("nil-program", b""), ("zero-byte-program", b"\x00"), ("path6", b"\x06"),
                              ("altered-program-data", changed), ("other-locked-program", decode(modules[other]))]
            require(all(identity(encode(tree))["module_tree_hash"] != identity(modules[alias])["module_tree_hash"] for _, tree in wrong_programs),
                    "negative data unexpectedly matches fixed commitment")
            wrong = [{"label": label, "solution": (tree, environment)} for label, tree in wrong_programs]
            outer = [{"label": "outer-nil", "solution": b""}, {"label": "outer-atom", "solution": b"\xff"}]
            for mode in ("off", "on"):
                for name in (alias + "_witness", "strict_unused_" + alias + "_witness"):
                    expected = canonical[alias] if not name.startswith("strict_unused_") else [
                        {"status": "accepted", "output_hex": "80"} if row["status"] == "accepted" else row for row in canonical[alias]]
                    require(self.batch(f"rust-{mode}-{name}-matching", artifacts[(mode, name)], matching) == expected,
                            "matching actual Rust differs from canonical exact output/typed failure")
                    require(self.batch(f"rust-{mode}-{name}-wrong-program", artifacts[(mode, name)], wrong)
                            == [{"status": "rejected", "code": "raise"}] * len(wrong), "fixed guard/order rejected code differs")
                    require(self.batch(f"rust-{mode}-{name}-outer-shape", artifacts[(mode, name)], outer)
                            == [{"status": "rejected", "code": "raise"}] * len(outer), "raw whole-environment shape contract differs")
        return {"canonical": canonical, "path6_direct_code": "path_into_atom",
            "scope": "Finite exact outputs and typed failures in actual canonical-first and both Rust modes. Matching-program/discarded Apply failure credit belongs only to multiplication. Quote lanes supply no such strictness observation."}

    def replay_mutant(self, artifacts):
        environment = self.origin.vectors["multiply"][1]["environment"]
        cases = [{"label": "matching-multiply", "solution": (decode(self.origin.modules["multiply"]), environment)},
                 {"label": "wrong-nil-program", "solution": (b"", environment)},
                 {"label": "wrong-path6-program", "solution": (b"\x06", environment)}]
        results, joined_baseline = {}, {}
        for mode in ("off", "on"):
            for name in self.exports:
                discarded = name.startswith("strict_unused_")
                expected = mutant_expected(self.mutant, discarded=discarded)
                actual = self.batch(f"mutant-{mode}-{name}", artifacts[(mode, name)], cases)
                require(actual == expected, "actual source mutant failed its exact distinguishing outcome control")
                references = [(f"rust-{mode}-{name}-matching", "positive"),
                              (f"rust-{mode}-{name}-wrong-program", "nil-program"),
                              (f"rust-{mode}-{name}-wrong-program", "path6")]
                original = []
                for label, case in references:
                    matches = [row for row in self.origin.observations if row["program"] == label and row["case"] == case]
                    require(len(matches) == 1 and "conclusive_outcome" in matches[0], "one actual baseline source observation required for mutant join")
                    original.append(matches[0]["conclusive_outcome"])
                require(actual != original, "actual source mutant must distinguish baseline behavior")
                results[mode + "/" + name] = actual
                joined_baseline[mode + "/" + name] = {"references": references, "actual_outcomes": original}
        return {"outcomes": results, "joined_actual_baseline": joined_baseline,
            "canonical_first_baseline_report": file_identity(self.origin.output / "report.json"),
            "mutation": self.mutation_record,
            "scope": "Actual source-body mutant, ordinary native validity, read-only regeneration conflict, subset acceptance, complete paired compiled packages and finite distinguishing VM observations; no generic optimizer, cryptographic or preservation proof."}


def finish(run, report, error):
    windows = {}
    for name, inventory in (("host-inputs", run.host_inputs), ("origin-inputs", run.origin_inputs), ("native-inputs", run.native_inputs),
                            ("vm-inputs", run.vm_inputs), ("facade-features", run.feature_inputs)):
        before, after = inventory.files(), inventory.snapshot()
        write_json(run.output / (name + "-before.json"), before)
        write_json(run.output / (name + "-after.json"), after)
        windows[name] = {"before": file_identity(run.output / (name + "-before.json")),
                         "after": file_identity(run.output / (name + "-after.json")), "status": after["status"]}
        if after["status"] != "matched" and error is None:
            error = ValueError(name + " changed or unreadable")
            report.update(status="failed", error=str(error))
    report["identity_windows"] = windows
    if hasattr(run, "native_before"):
        try:
            after = run.quiet_inputs()
            write_json(run.output / "native-quiet-after.json", after)
            require(after == run.native_before, "complete native/extraction implementation/tool/source window changed")
            report["native_quiet_window"] = {"before": file_identity(run.output / "native-quiet-before.json"),
                "after": file_identity(run.output / "native-quiet-after.json"), "status": "matched"}
        except Exception as caught:
            report["native_quiet_window_error"] = f"{type(caught).__name__}: {caught}"
            if error is None:
                error = caught
                report.update(status="failed", error=report["native_quiet_window_error"])
    try:
        retained = run.retained_inputs()
        write_json(run.output / "retained-final.json", retained)
        report["retained_final_inventory"] = file_identity(run.output / "retained-final.json")
    except Exception as caught:
        report["retained_inventory_error"] = f"{type(caught).__name__}: {caught}"
        if error is None:
            error = caught
            report.update(status="failed", error=report["retained_inventory_error"])
    if hasattr(run, "before"):
        try:
            after = run.quiet_inputs()
            write_json(run.output / "quiet-after.json", after)
            require(after == run.before, "actual source/tool/pin/store quiet window changed")
            if hasattr(run, "retained_before"):
                after = run.retained_inputs()
                write_json(run.output / "retained-after.json", after)
                require(after == run.retained_before, "complete retained source/IR/blob/package/feature inventory changed")
            report["quiet_window"] = "exact source/tool/pin and complete retained transport before/after identities checked"
        except Exception as caught:
            report["quiet_window_error"] = f"{type(caught).__name__}: {caught}"
            if error is None:
                error = caught
                report.update(status="failed", error=report["quiet_window_error"])
    report.update(commands=run.commands, vm_observations=run.observations, vm_input_batches=run.vm_batches,
                  facade_feature_records=run.feature_records, resource_or_infrastructure_rejection_credit=0)
    try:
        report["command_log_inventory"] = check_command_logs(run.output, run.commands)
    except Exception as caught:
        report["command_log_error"] = f"{type(caught).__name__}: {caught}"
        if error is None:
            error = caught
            report.update(status="failed", error=report["command_log_error"])
    report["actual_vm_observations"] = len(run.observations)
    report["accepted"] = sum(row.get("conclusive_outcome", {}).get("status") == "accepted" for row in run.observations)
    report["exact_typed_program_rejections"] = sum(row.get("conclusive_outcome", {}).get("status") == "rejected" for row in run.observations)
    if error is None:
        report["status"] = "passed"
    else:
        report["complete_workflow_credit"] = False
    write_json(run.output / "report.json", report)
    return error


def execute(run):
    report = {"schema_version": 1, "status": "running", "vm_failure_protocol": PROTOCOL,
        "scope": "Actual explicit local witness source generation, ordinary Rust checking, complete source receipts and finite foreign-program composition controls. Existing inner-puzzle pattern; changed funding ABI/hash. Logical tree commitment under SHA assumptions, not original wire/source authentication. No nominal ABI, complete spend, cryptographic theorem, all-Chialisp Rust implementation or compiler-preservation credit."}
    error = None
    try:
        run.materialize()
        if run.origin is None:
            run.modules, run.vectors = fixtures()
            report["generation"] = run.resolve_and_generate(run.modules)
        report["native"] = run.native_phase()
        if run.sdk:
            report["sdk_body_control"] = run.extract_sdk_rejection()
        else:
            run.extract()
            report["bundles"], artifacts = run.inspect()
            run.retained_before = run.retained_inputs()
            write_json(run.output / "retained-before.json", run.retained_before)
            report["comparison"] = (run.replay(run.modules, run.vectors, artifacts) if run.origin is None else run.replay_mutant(artifacts))
        report["compiler_queries"] = {"driver_declaration": run.driver_compiler_declaration,
            "pinned_rustc_vV": run.compiler, "fields": run.compiler_fields,
            "scope": "Two actual cross-checked queries; declarations are not installed-byte authentication."}
    except Exception as caught:
        error = caught
        report.update(status="inconclusive" if isinstance(caught, Inconclusive) else "failed", error=f"{type(caught).__name__}: {caught}")
    error = finish(run, report, error)
    return report, error


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execute", action="store_true", required=True)
    parser.add_argument("--output", type=Path, required=True)
    for name, binary in (("registry-cli", "cargo-chialisp-oxide"), ("cli", "chialisp-oxide"), ("driver", "chialisp-oxide-driver")):
        parser.add_argument("--" + name, type=Path, default=ROOT / "target/debug" / binary)
    parser.add_argument("--rustup", type=Path, default=Path(shutil.which("rustup") or "/nonexistent/rustup"))
    args = parser.parse_args()
    for name in ("registry_cli", "cli", "driver", "rustup"):
        setattr(args, name, getattr(args, name).resolve(strict=True))
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    coordinator = {"schema_version": 1, "status": "running", "lanes": [],
                   "scope": "Actual baseline, six local generated-source mutants and one copied SDK-body mutant, separately retained. Resources/infrastructure inconclusive; zero execution credit from private experiments."}
    # Reports are immutable after each lane. Freeze their complete source,
    # retained transport, input inventories and command logs across later lanes.
    completed = []
    error = None
    baseline = None
    try:
        for name in ("baseline", *MUTANTS, "sdk_body_tamper"):
            child_args = argparse.Namespace(**vars(args))
            child_args.output = output / name
            run = WitnessRun(child_args, mutant=name if name in MUTANTS else None,
                             origin=baseline, sdk=name == "sdk_body_tamper")
            report, failure = execute(run)
            coordinator["lanes"].append({"name": name, "status": report["status"], "report": file_identity(run.output / "report.json"),
                "actual_commands": len(run.commands), "actual_vm_observations": len(run.observations)})
            write_json(output / "report.json", coordinator)
            if failure is not None:
                raise failure
            if baseline is None:
                baseline = run
            completed.append(run)
        coordinator["status"] = "passed"
    except Exception as caught:
        error = caught
        coordinator.update(status="inconclusive" if isinstance(caught, Inconclusive) else "failed", error=f"{type(caught).__name__}: {caught}", complete_workflow_credit=False)
    finally:
        for run in completed:
            try:
                require(run.quiet_inputs() == run.before, "completed lane source/tool quiet window changed during later lane")
                require(run.quiet_inputs() == run.native_before, "completed lane full native source/tool window changed during later lane")
                if hasattr(run, "retained_before"):
                    require(run.retained_inputs() == run.retained_before, "completed lane retained packet changed during later lane")
                require(all(inventory.snapshot()["status"] == "matched" for inventory in
                    (run.host_inputs, run.origin_inputs, run.native_inputs, run.vm_inputs, run.feature_inputs)), "completed lane inputs changed during later lane")
                row = next(row for row in coordinator["lanes"] if row["name"] == run.output.name)
                require(file_identity(run.output / "report.json") == row["report"], "completed lane report changed")
                retained_report = decode_json(read(run.output / "report.json"))
                require(check_command_logs(run.output, run.commands) == retained_report["command_log_inventory"],
                        "completed lane command manifest or raw stdout/stderr changed during later lane")
            except Exception as caught:
                coordinator.setdefault("final_window_errors", []).append(f"{run.output.name}: {type(caught).__name__}: {caught}")
                if error is None:
                    error = caught
                    coordinator.update(status="failed", complete_workflow_credit=False)
        coordinator["resource_or_infrastructure_rejection_credit"] = 0
        coordinator["actual_commands"] = sum(row["actual_commands"] for row in coordinator["lanes"])
        coordinator["actual_vm_observations"] = sum(row["actual_vm_observations"] for row in coordinator["lanes"])
        write_json(output / "report.json", coordinator)
    if error is not None:
        print(f"{coordinator['status']}: {error}; partial report: {output / 'report.json'}", file=sys.stderr)
        return 1
    print(f"registry witness: {coordinator['actual_commands']} actual commands, {coordinator['actual_vm_observations']} finite VM observations; no spend/proof/preservation credit")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
