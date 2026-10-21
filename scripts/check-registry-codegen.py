#!/usr/bin/env python3
"""Explicitly execute the local registry → generated Rust → CLVM workflow.

Root must finish the shared host build and freeze sources before --execute.
This gate serializes ordinary native Cargo, independent Chialisp compilation,
then actual subset/build/receipt admission and finite retained-byte VM replay.
It never rebuilds the shared driver. A failed partial report is not success.
"""
import argparse
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import subprocess
import sys
import time
import tomllib

import cargo_bundle
from cargo_source_receipt import decode_json
from registry_codegen_evidence import (
    CRATE, EXPORTS, MAX_JSON, Construction, FrozenFiles, Inconclusive, decode, digest, encode,
    facade_feature_record, file_identity, identity, inspect_compiled_artifact, normalized, proper, read, require, text, unhex, write_json,
)
from source_walk import source_files
from vm_failures import PROTOCOL

ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / "scripts/registry_codegen_fixture"
FIXTURE = ROOT / "crates/chialisp-oxide-package/tests/fixtures/mainnet-standard"
PACKAGE = "registry-generated-fixture"
MODULE_SHA = "21747ef070376012f22f7b426edaa36a244a77f8825d13d5529963b921ad1da7"
MODULE_HASH = "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52"
KEY_HEX = "9415e2b4b30977380e167096714abfdd5a594f6326dc278aa895e6bd6901953b9d8d37126e5411027eda81be8b16b97f"
RECIPE = "chialisp-oxide-generated-tree/v1"
MAX_COST = 10_000_000
SOURCE_PATHS = ("Cargo.toml", "Cargo.lock", "src/lib.rs", "src/key.rs",
                "src/generated/multiply.rs", "src/generated/standard.rs",
                "src/generated/atom255.rs", "src/generated/atom256.rs",
                "examples/compile-reference.rs", "dependencies.lock")


def check_compiler_queries(driver_declaration, compiler):
    """Cross-check two actual query protocols without inventing driver fields.

    The driver's --compiler-version prints its one-line build declaration.
    The pinned rustc -vV command supplies the independently queried complete
    identity required by the bundle reader. Neither declaration authenticates
    installed bytes or establishes compiler preservation.
    """
    require(type(driver_declaration) is str and len(driver_declaration) <= 4096
            and driver_declaration.endswith("\n") and len(driver_declaration.splitlines()) == 1,
            "one-line actual driver compiler declaration required")
    require(type(compiler) is str and len(compiler) <= 16384 and compiler.endswith("\n")
            and "\r" not in compiler, "complete actual pinned rustc -vV query required")
    lines = compiler.splitlines()
    require(len(lines) == 7 and lines[0].startswith("rustc ")
            and driver_declaration == lines[0] + "\n", "driver declaration differs from actual pinned rustc")
    fields = {}
    for line in lines[1:]:
        key, separator, value = line.partition(": ")
        require(separator and key not in fields and value and value == value.strip(),
                "complete unique actual compiler identity fields required")
        fields[key] = value
    require(set(fields) == {"binary", "commit-hash", "commit-date", "host", "release", "LLVM version"}
            and fields["binary"] == "rustc" and re.fullmatch("[0-9a-f]{40}", fields["commit-hash"])
            and re.fullmatch(r"\d{4}-\d{2}-\d{2}", fields["commit-date"])
            and lines[0].split(" ", 2)[1] == fields["release"], "complete actual compiler identity mismatch")
    return fields


class Run:
    def __init__(self, args):
        self.args, self.output = args, args.output.resolve()
        self.output.mkdir(parents=True, exist_ok=False)
        self.project = self.output / "consumer"
        self.commands, self.observations = [], []
        self.vm_inputs, self.feature_inputs = FrozenFiles(), FrozenFiles(max_files=8, max_bytes=8 * 1024 * 1024)
        self.vm_batches, self.feature_records = [], {}
        self.environment = dict(os.environ, CARGO_NET_OFFLINE="true", PYTHONDONTWRITEBYTECODE="1")
        for key in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER",
                    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "RUSTC", "CARGO", "RUSTUP_TOOLCHAIN",
                    "CHIALISP_OXIDE_SOURCE_CAPTURE", "CHIALISP_OXIDE_SOURCE_CAPTURE_DIR"):
            self.environment.pop(key, None)
        for key in list(self.environment):
            if key.startswith("CHIALISP_OXIDE_SOURCE_"):
                self.environment.pop(key)
        self.environment["CHIALISP_OXIDE_DRIVER"] = str(args.driver)
        self.environment["CARGO_TARGET_DIR"] = str(self.output / "compiler-target")
        self.offline = self.environment | {
            "PATH": "", "CARGO": "/nonexistent/cargo", "RUSTC": "/nonexistent/rustc",
            "RUSTUP_TOOLCHAIN": "invalid-no-toolchain-query", "CHIALISP_OXIDE_DRIVER": "/nonexistent/driver",
            "RUSTC_WRAPPER": "/nonexistent/wrapper", "RUSTC_WORKSPACE_WRAPPER": "/nonexistent/workspace-wrapper",
        }

    def command(self, label, argv, *, offline=False, expected=0, timeout=180, json_result=False):
        """Stream bounded outputs to files; retain terminal status on every path."""
        require(all(row["label"] != label for row in self.commands), "unique child command labels required")
        row = {"label": label, "argv": list(map(str, argv)), "offline_empty_path": offline,
               "status": "starting", "timeout_seconds": timeout, "output_limit_bytes_each": MAX_JSON}
        self.commands.append(row)
        write_json(self.output / "commands.json", self.commands)
        started, failure = time.monotonic(), None
        paths = [self.output / (label + ".stdout"), self.output / (label + ".stderr")]
        try:
            with paths[0].open("xb") as out, paths[1].open("xb") as err:
                with subprocess.Popen(row["argv"], cwd=ROOT, env=self.offline if offline else self.environment,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
                    selector = selectors.DefaultSelector()
                    counts = [0, 0]
                    for index, stream in enumerate((child.stdout, child.stderr)):
                        selector.register(stream, selectors.EVENT_READ, index)
                    try:
                        while selector.get_map():
                            if time.monotonic() - started > timeout:
                                raise Inconclusive(f"{label}: child timeout")
                            for key, _ in selector.select(timeout=0.25):
                                chunk = os.read(key.fd, 65536)
                                if not chunk:
                                    selector.unregister(key.fileobj)
                                    continue
                                index = key.data
                                require(counts[index] + len(chunk) <= MAX_JSON, f"{label}: child output bound")
                                (out, err)[index].write(chunk)
                                counts[index] += len(chunk)
                        row.update(status="completed", exit_code=child.wait(timeout=5))
                    except Exception:
                        child.kill()
                        row.update(status="terminated", exit_code=child.wait(timeout=5))
                        raise
                    finally:
                        selector.close()
        except Exception as error:
            failure = error
            row.update(error=f"{type(error).__name__}: {error}")
        row["seconds"] = round(time.monotonic() - started, 3)
        for key, path in zip(("stdout", "stderr"), paths, strict=True):
            if path.exists():
                row[key] = file_identity(path)
        write_json(self.output / "commands.json", self.commands)
        if failure is not None:
            raise Inconclusive(f"{label}: child launch/transport/output failure") from failure
        require(row["exit_code"] == expected, f"{label}: expected exit {expected}, got {row['exit_code']}; see retained stderr")
        raw = read(paths[0])
        return decode_json(raw) if json_result else raw

    def registry(self, label, *arguments, expected=0):
        return self.command(label, [self.args.registry_cli, "registry", *arguments, "--json"],
                            offline=True, expected=expected, json_result=True)

    def materialize(self):
        shutil.copytree(TEMPLATE, self.project)
        manifest = read(self.project / "Cargo.toml").decode()
        parsed = tomllib.loads(manifest)
        require(parsed["package"]["name"] == PACKAGE and parsed["workspace"] == {}, "isolated ordinary Cargo template required")
        rewrites = {}
        for crate in ("chialisp-oxide", "chialisp-oxide-reference"):
            old = f'"../../crates/{crate}"'
            new = json.dumps(str(ROOT / "crates" / crate))
            require(manifest.count(old) == (2 if crate == "chialisp-oxide" else 1), "closed manifest path rewrite inventory")
            manifest = manifest.replace(old, new)
            rewrites[crate] = {"from": old, "to": new}
        (self.project / "Cargo.toml").write_text(manifest, encoding="utf-8")
        (self.project / "src/generated").mkdir()
        write_json(self.output / "manifest-recipe.json", {
            "scope": "Explicit local path rewrite only, no discovered Cargo dependencies.",
            "template": file_identity(TEMPLATE / "Cargo.toml"), "rewrites": rewrites,
            "materialized": file_identity(self.project / "Cargo.toml")})

    def publish_plain(self, alias, raw):
        package = {"schema_version": 1, "kind": "clvm_artifact",
                   "artifact": {"identity": identity(raw), "clvm_hex": raw.hex()},
                   "rust": None, "source_scope": "not_recorded", "sources": [], "provenance": None}
        path = self.output / (alias + ".package.json")
        write_json(path, package)
        summary = self.registry("publish-" + alias, "publish", "--package", path, "--registry", self.output / "registry")["package"]
        require(summary["object_id"] == digest(read(path)) and summary["blob_sha256"] == digest(raw)
                and summary["module_tree_hash"] == identity(raw)["module_tree_hash"], "published exact package identity differs")
        added = self.registry("pin-" + alias, "add", "--registry", self.output / "registry", "--lock", self.project / "dependencies.lock",
                              "--name", alias, "--module-hash", summary["module_tree_hash"], "--object-id", summary["object_id"])
        require(added["status"] == "added" and added["pin"]["binding"] is None, "exact opaque dependency pin missing")
        return summary

    def generate(self, modules):
        summaries = {alias: self.publish_plain(alias, raw) for alias, raw in modules.items()}
        records = {}
        lock_sha = digest(read(self.project / "dependencies.lock"))
        for alias, module in modules.items():
            source = self.project / "src/generated" / (alias + ".rs")
            arguments = ["generate-rust", "--registry", self.output / "registry", "--lock", self.project / "dependencies.lock",
                         "--name", alias, "--output", source]
            first = self.registry("generate-" + alias, *arguments)
            raw = read(source, 262144)
            record = first["generation"]
            require(first["status"] == "created" and record["recipe"] == RECIPE and record["schema_version"] == 1
                    and record["lock_sha256"] == lock_sha and record["selected_alias"] == alias
                    and record["object_id"] == summaries[alias]["object_id"] and record["artifact"] == identity(module)
                    and record["source_sha256"] == digest(raw) and record["source_bytes"] == len(raw), "actual generation identity/record mismatch")
            for suffix, status, extra in (("repeat", "already_present", []), ("check", "checked", ["--check"])):
                repeated = self.registry(f"generate-{alias}-{suffix}", *arguments, *extra)
                require(repeated["status"] == status and repeated["generation"] == record and read(source) == raw,
                        "generation repeat/check altered exact source or identity")
            # A source-only mutation is a regeneration control, not a compiler
            # diagnostic or newly executed mutant policy.
            source.write_bytes(raw + b"\n// constructed source mutation\n")
            failed = self.registry("generate-" + alias + "-changed-check", *arguments, "--check", expected=1)
            require(failed["status"] == "error" and failed["kind"] == "conflict"
                    and read(source) == raw + b"\n// constructed source mutation\n", "read-only altered-source conflict missing")
            source.write_bytes(raw)
            records[alias] = {"generation": record, "source": file_identity(source), "package": summaries[alias]}
        return records

    def snapshot(self):
        return {"mode": "cargo_library", "package": PACKAGE, "manifest": "Cargo.toml", "library": "src/lib.rs",
                "exports": {name: {"entry": CRATE + "::" + name, "source": "src/lib.rs"} for name in EXPORTS},
                "sources": [{"path": path, "sha256": digest(read(self.project / path))} for path in SOURCE_PATHS]}

    def quiet_inputs(self):
        roots = ["crates/chialisp-oxide", "crates/chialisp-oxide-macros", "crates/chialisp-oxide-driver",
                 "crates/chialisp-oxide-ir", "crates/chialisp-oxide-package", "crates/cargo-chialisp-oxide",
                 "crates/chialisp-oxide-vm", "crates/chialisp-oxide-reference", "crates/chialisp-oxide-cli",
                 "vendor/rue-lir", "scripts/registry_codegen_fixture"]
        paths = source_files(ROOT, roots, suffixes=(".rs", ".toml", ".json", ".md"))
        paths += source_files(ROOT, [FIXTURE.relative_to(ROOT).as_posix()], suffixes=None)
        paths += ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
                  "scripts/check-registry-codegen.py", "scripts/registry_codegen_evidence.py",
                  "scripts/cargo_bundle.py", "scripts/cargo_source_receipt.py", "scripts/cargo_authoring.py",
                  "scripts/source_walk.py", "scripts/vm_failures.py"]
        return {"repository_sources": [file_identity(ROOT / path) for path in sorted(set(paths))],
                "consumer": self.snapshot(), "binaries": [file_identity(path, 512 * 1024 * 1024)
                    for path in (self.args.registry_cli, self.args.cli, self.args.driver)]}

    def retained_inputs(self):
        paths = source_files(self.output, ["bundle-off", "bundle-on", "registry", "compiled-registry"], suffixes=None)
        paths += [Path(row["path"]).relative_to(self.output).as_posix() for row in self.feature_inputs.files()]
        paths = sorted(set(paths))
        require(sum((self.output / path).stat().st_size for path in paths) <= 64 * 1024 * 1024,
                "retained transport aggregate byte bound")
        return [file_identity(self.output / path) for path in paths]

    def native_and_extract(self):
        manifest = self.project / "Cargo.toml"
        cargo = [self.args.rustup, "run", self.toolchain, "cargo"]
        before_format = {alias: read(self.project / "src/generated" / (alias + ".rs"))
                         for alias in ("multiply", "standard", "atom255", "atom256")}
        self.command("ordinary-cargo-fmt", [*cargo, "fmt", "--manifest-path", manifest])
        for alias, original in before_format.items():
            source = self.project / "src/generated" / (alias + ".rs")
            require(read(source) == original, "ordinary Cargo formatting changed generated source bytes")
            checked = self.registry("post-fmt-generation-check-" + alias, "generate-rust", "--registry", self.output / "registry",
                "--lock", self.project / "dependencies.lock", "--name", alias, "--output", source, "--check")
            require(checked["status"] == "checked" and checked["generation"]["source_sha256"] == digest(original),
                    "post-format actual source regeneration check failed")
        self.command("lock", [*cargo, "generate-lockfile", "--manifest-path", manifest, "--offline"])
        native = ["--manifest-path", manifest, "--locked", "--offline", "--target-dir", self.output / "native-target"]
        self.command("native-check", [*cargo, "check", *native], timeout=600)
        tests = self.command("native-tests", [*cargo, "test", "--lib", *native], timeout=600)
        require(b"test result: ok. 3 passed; 0 failed; 0 ignored;" in tests,
                "three actual native test groups required")
        return cargo, native

    def source_mutation(self, cargo, native):
        source = self.project / "src/generated/multiply.rs"
        original = read(source)
        old, new = b"Tree::Atom(&[0x12])", b"Tree::Atom(&[0x10])"
        require(original.count(old) == 1, "fixed multiplier opcode source mutation site must be unique")
        mutant = original.replace(old, new, 1)
        try:
            source.write_bytes(mutant)
            self.command("native-check-source-mutant", [*cargo, "check", "--lib", *native], timeout=600)
            failed = self.registry("source-body-mutant-check", "generate-rust", "--registry", self.output / "registry",
                "--lock", self.project / "dependencies.lock", "--name", "multiply", "--output", source, "--check", expected=1)
            require(failed["status"] == "error" and failed["kind"] == "conflict" and read(source) == mutant,
                    "ordinary Rust-valid changed opcode must fail read-only regeneration")
            return {"original_sha256": digest(original), "mutant_sha256": digest(mutant),
                    "recipe": "one exact generated Tree atom changes multiply opcode 0x12 to add opcode 0x10",
                    "scope": "Actual ordinary native Cargo check and read-only regeneration conflict only; no mutant VM, subset, signature or spend credit."}
        finally:
            source.write_bytes(original)
            require(read(source) == original, "restore exact generated source before extraction")

    def extract(self):
        self.driver_compiler_declaration = self.command("actual-driver-compiler-query", [self.args.driver, "--compiler-version"]).decode()
        self.compiler = self.command("actual-pinned-rustc-query", [self.args.rustup, "run", self.toolchain, "rustc", "-vV"]).decode()
        self.compiler_fields = check_compiler_queries(self.driver_compiler_declaration, self.compiler)
        self.before = self.quiet_inputs()
        write_json(self.output / "quiet-before.json", self.before)
        manifest = self.project / "Cargo.toml"
        self.command("subset-check", [self.args.registry_cli, "check", "--manifest-path", manifest, "--locked"], timeout=600)
        for mode in ("off", "on"):
            args = [self.args.registry_cli, "build", "--manifest-path", manifest, "--locked", "--output-dir", self.output / ("bundle-" + mode)]
            if mode == "off":
                args.append("--no-optimize")
            self.command("build-" + mode, args, timeout=600)

    def inspect(self, modules):
        before = self.snapshot()
        results, artifacts = {}, {}
        for mode in ("off", "on"):
            lane = {}
            for name in EXPORTS:
                export = cargo_bundle.load_declared_export(self.output / ("bundle-" + mode), root=self.project,
                    authoring=before, selected_entry=CRATE + "::" + name, compiler=self.compiler,
                    driver_sha256=next(row["sha256"] for row in self.before["binaries"] if row["path"] == str(self.args.driver)), toolchain=self.toolchain,
                    current_authoring=self.snapshot, optimize=mode == "on",
                    cargo_options=["--manifest-path", str(self.project / "Cargo.toml"), "--locked"])
                require(export.manifest["schema_version"] == 2, "complete schema2 source receipt required")
                expected = {"package/Cargo.toml", "package/Cargo.lock", "package/src/lib.rs", "package/src/key.rs"}
                expected |= {"package/src/generated/" + alias + ".rs" for alias in modules}
                require({source.label for source in export.source_files} == expected, "complete actual selected-library source inventory mismatch")
                for source in export.source_files:
                    require(source.contents == read(self.project / source.label.removeprefix("package/")), "captured source differs from actual generated/copied source")
                    if source.label.endswith(".rs"):
                        require(source.kinds == ("compiler_text",) and source.compiler_read_count >= 1, "actual generated compiler-read bytes required")
                unit = export.source_receipt["unit"]
                require(unit == {"crate_name": CRATE, "analysis_target": export.manifest["analysis_target"],
                    "features": [], "library_source": "package/src/lib.rs"}, "actual selected-unit features/library mismatch")
                require(export.source_receipt["snapshots"] == {
                    "package_manifest": "package/Cargo.toml", "workspace_manifest": "package/Cargo.toml",
                    "lockfile": "package/Cargo.lock", "build_script": None}, "explicit named launcher snapshots required")
                selected = next(row for row in export.manifest["entries"] if row["entry"] == CRATE + "::" + name)
                raw = unhex(selected["artifact"]["clvm_hex"])
                compiled = inspect_compiled_artifact(raw)
                require(compiled["identity"]["module_tree_hash"] == selected["artifact"]["tree_hash"],
                        "independent compiled artifact tree hash differs from actual bundle metadata")
                alias = "multiply" if name == "strict_unused_apply" else name
                prefix = b"\x02" if alias == "multiply" else bytes.fromhex(KEY_HEX) if alias == "standard" else None
                shape = Construction(export.program, name, modules[alias], prefix).inspect(name == "strict_unused_apply")
                artifact = export.manifest_path.parent / selected["clvm"]
                require(read(artifact) == (raw.hex() + "\n").encode(), "retained compiled bytes mismatch")
                lane[name] = {"manifest": file_identity(export.manifest_path), "ir": file_identity(export.path),
                              "artifact": {**compiled["identity"], "file": file_identity(artifact)},
                              "compiled_output_inspection": compiled, "construction": shape,
                              "source_receipt": export.source_receipt,
                              "sources": [{"label": source.label, "sha256": source.sha256, "bytes": len(source.contents),
                                          "kinds": list(source.kinds), "compiler_read_count": source.compiler_read_count} for source in export.source_files]}
                artifacts[(mode, name)] = artifact
                published = self.registry(f"admit-{mode}-{name}", "publish", "--bundle", export.manifest_path,
                                          "--entry", CRATE + "::" + name, "--registry", self.output / "compiled-registry")
                summary = published["package"]
                require(summary["kind"] == "rust_cargo_export" and summary["blob_sha256"] == compiled["identity"]["blob_sha256"]
                        and summary["module_tree_hash"] == compiled["identity"]["module_tree_hash"]
                        and summary["source_scope"] == "declared_files_only" and summary["source_files"] == len(expected),
                        "actual complete compiled bundle/source admission differs")
                package_path = self.output / "compiled-registry/objects" / (summary["object_id"] + ".json")
                verified = self.registry(f"verify-{mode}-{name}", "verify", "--package", package_path)
                require(verified["package"] == summary and digest(read(package_path)) == summary["object_id"], "actual exact compiled package reread mismatch")
                lane[name]["package"] = summary
            fingerprints = self.output / "compiler-target/chialisp-oxide/cargo" / export.manifest["driver_sha256"] / export.manifest["analysis_target"] / "debug/.fingerprint"
            paths = sorted(fingerprints.glob("chialisp-oxide-*/lib-chialisp_oxide.json"))
            require(len(paths) == 1, "one actual extraction facade feature record required")
            record = facade_feature_record(paths[0])
            require(self.feature_inputs.observe(paths) == [record["file"]], "checked facade metadata changed during retention")
            self.feature_records[mode] = record
            results[mode] = lane
        require(all(results["off"][name]["ir"]["sha256"] == results["on"][name]["ir"]["sha256"] for name in EXPORTS),
                "same actual Rust source extraction must retain the same paired IR")
        return results, artifacts

    def batch(self, label, program, cases, framed=False):
        inputs = self.output / (label + ".inputs.json")
        write_json(inputs, [text(case["environment"] if framed else case["solution"]) for case in cases])
        batch = {"label": label, "program_path": str(program), "solutions_path": str(inputs),
                 "before": self.vm_inputs.observe([program, inputs]), "status": "starting"}
        self.vm_batches.append(batch)
        write_json(self.output / "vm-inputs-before.json", {"scope": "First actual bytes of each path, deduplicated; no authentication claim.", "files": self.vm_inputs.files()})
        write_json(self.output / "vm-input-batches.json", self.vm_batches)
        failure = None
        try:
            rows = self.command(label, [self.args.cli, "run-batch", program, "--solutions", inputs,
                                       "--max-cost", MAX_COST], offline=True, json_result=True)
        except Exception as error:
            failure = error
            batch["child_error"] = f"{type(error).__name__}: {error}"
        finally:
            try:
                batch["after"] = [file_identity(row["path"]) for row in batch["before"]]
                require(batch["after"] == batch["before"], "actual VM program or solutions changed during child execution")
                batch["status"] = "inputs_matched" if failure is None else "child_failed_inputs_matched"
            except Exception as error:
                batch.update(status="inputs_failed", input_error=f"{type(error).__name__}: {error}")
                if failure is None:
                    failure = error
            write_json(self.output / "vm-input-batches.json", self.vm_batches)
        if failure is not None:
            raise failure
        require(type(rows) is list and len(rows) == len(cases), "complete actual batch required")
        outcomes = []
        for case, execution in zip(cases, rows, strict=True):
            record = {"program": label, "case": case["label"], "execution": execution}
            self.observations.append(record)
            record["conclusive_outcome"] = normalized(execution, label + "/" + case["label"])
            outcomes.append(record["conclusive_outcome"])
        return outcomes

    def replay(self, modules, cases, artifacts, standard_curry):
        canonical = {}
        for alias, vectors in cases.items():
            original = self.output / (alias + ".canonical.hex")
            original.write_text(modules[alias].hex() + "\n", encoding="ascii")
            outcomes = self.batch("canonical-original-" + alias, original, vectors, framed=True)
            for case, outcome in zip(vectors, outcomes, strict=True):
                if case.get("expected") is not None:
                    require(outcome == case["expected"], "canonical result differs from independent expectation")
            if alias in ("multiply", "standard"):
                curry = standard_curry if alias == "standard" else encode(proper([b"\x02", (b"\x01", decode(modules[alias])), proper([b"\x04", (b"\x01", b"\x02"), b"\x01"])]))
                path = self.output / (alias + ".curried.hex")
                path.write_text(curry.hex() + "\n", encoding="ascii")
                require(self.batch("canonical-curried-" + alias, path, vectors) == outcomes, "actual original and independently framed curry differ")
            canonical[alias] = outcomes
        write_json(self.output / "new-actual-canonical-outcomes.json", canonical)
        for mode in ("off", "on"):
            for alias, vectors in cases.items():
                require(self.batch(f"rust-{mode}-{alias}", artifacts[(mode, alias)], vectors) == canonical[alias], "Rust output or exact typed code differs from actual canonical comparators")
            strict = self.batch("rust-" + mode + "-strict-unused", artifacts[(mode, "strict_unused_apply")], cases["multiply"])
            expected = [({"status": "accepted", "output_hex": "80"} if row["status"] == "accepted" else row) for row in canonical["multiply"]]
            require(strict == expected, "discarded strict application lost its actual failure behavior")
        return canonical


def fixtures():
    provenance = decode_json(read(FIXTURE / "provenance.json"))
    require({path.name for path in FIXTURE.iterdir()} == {row["path"] for row in provenance["files"]} | {"provenance.json"}, "complete retained fixture inventory required")
    for row in provenance["files"]:
        raw = read(FIXTURE / row["path"], 65536)
        require(len(raw) == row["bytes"] and digest(raw) == row["sha256"], "retained fixture declared bytes mismatch")
    expected = decode_json(read(FIXTURE / "expected.json"))
    module, reveal = unhex(expected["module_hex"]), unhex(expected["puzzle_reveal_hex"])
    require(len(module) == 227 and digest(module) == MODULE_SHA and identity(module)["module_tree_hash"] == MODULE_HASH
            and expected["arguments"][0]["hex"] == KEY_HEX, "fixed retained module/key identity mismatch")
    key = bytes.fromhex(KEY_HEX)
    curry = encode(proper([b"\x02", (b"\x01", decode(module)), proper([b"\x04", (b"\x01", key), b"\x01"])]))
    require(curry == reveal and identity(reveal)["module_tree_hash"] == expected["puzzle_tree_hash"], "independent exact retained recurry mismatch")
    modules = {"multiply": bytes.fromhex("ff12ff02ff0580"), "standard": module}
    modules.update({f"atom{size}": encode((b"\x01", b"\xa5" * size)) for size in (255, 256)})
    multiply = []
    def integer(value):
        if value == 0:
            return b""
        width = (value.bit_length() + 8) // 8
        return value.to_bytes(width, "big", signed=True)
    for label, value, tail, answer in (
        ("zero", b"", b"", 0), ("positive", b"\x15", b"", 42),
        ("negative", b"\xff", b"", -2), ("signed-boundary", b"\x00\x80", b"", 256),
        ("unbounded-clvm", integer(1 << 128), b"", 1 << 129),
        ("padded-zero", b"\x00\x00", b"", 0), ("padded-positive", b"\x00\x15", b"", 42),
        ("extra-field", b"\x15", proper([b"\x7f"]), 42), ("improper-tail", b"\x15", b"\xff\x80", 42)):
        solution = (value, tail)
        multiply.append({"label": label, "solution": solution, "environment": (b"\x02", solution),
                         "expected": {"status": "accepted", "output_hex": encode(integer(answer)).hex()}})
    for label, solution in (("missing", b""), ("scalar", b"\x01"), ("pair-is-not-integer", ((b"\x01", b""), b""))):
        multiply.append({"label": label, "solution": solution, "environment": (b"\x02", solution), "expected": None})
    standard = []
    identity_program, quoted_nil = b"\x01", (b"\x01", b"")
    for label, program in (("identity-nil", identity_program), ("quoted-nil", quoted_nil)):
        solution = proper([b"", program, b""])
        output = proper([proper([b"\x32", key, bytes.fromhex(identity(encode(program))["module_tree_hash"])])])
        standard.append({"label": label, "solution": solution, "environment": (key, solution),
                         "expected": {"status": "accepted", "output_hex": encode(output).hex()}})
    observed = decode(unhex(expected["solution_hex"]))
    delegated = decode(unhex(expected["solution_fields"]["delegated_puzzle_hex"]))
    require(type(delegated) is tuple and delegated[0] == b"\x01", "observed delegated program must quote exact conditions")
    require(identity(encode(delegated))["module_tree_hash"] == expected["solution_fields"]["delegated_puzzle_tree_hash"],
            "independent retained delegated tree hash differs")
    signature = proper([b"\x32", key, bytes.fromhex(expected["solution_fields"]["delegated_puzzle_tree_hash"])])
    standard.append({"label": "retained-observed-solution", "solution": observed, "environment": (key, observed),
                     "expected": {"status": "accepted", "output_hex": encode((signature, delegated[1])).hex()}})
    for label, solution in (("empty", b""), ("scalar", b"\x01"), ("missing-third", proper([b"", b"\x01"])),
                            ("delegated-raise", proper([b"", proper([b"\x08"]), b""])),
                            ("improper-solution-tail", (b"", (b"\x01", b"\x01"))),
                            ("extra-fourth-field", proper([b"", b"\x01", b"", b"\x2a"]))):
        standard.append({"label": label, "solution": solution, "environment": (key, solution), "expected": None})
    cases = {"multiply": multiply, "standard": standard}
    for size in (255, 256):
        cases[f"atom{size}"] = [{"label": label, "solution": environment, "environment": environment,
            "expected": {"status": "accepted", "output_hex": encode(b"\xa5" * size).hex()}}
            for label, environment in (("nil", b""), ("nonutf8-atom", b"\xff\x80"), ("improper-pair", (b"\x00", b"\xff")))]
    return modules, cases, reveal


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execute", action="store_true", required=True, help="explicitly launch coordinated native/compiler/VM commands")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--registry-cli", type=Path, default=ROOT / "target/debug/cargo-chialisp-oxide")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/chialisp-oxide")
    parser.add_argument("--driver", type=Path, default=ROOT / "target/debug/chialisp-oxide-driver")
    parser.add_argument("--rustup", type=Path, default=Path(shutil.which("rustup") or "/nonexistent/rustup"))
    args = parser.parse_args()
    for name in ("registry_cli", "cli", "driver", "rustup"):
        setattr(args, name, getattr(args, name).resolve(strict=True))
    run = Run(args)
    report = {"schema_version": 1, "status": "running", "vm_failure_protocol": PROTOCOL,
              "scope": "Actual explicit local registry generation, native Cargo tests, subset checking, complete source receipts, finite canonical/Rust VM comparisons. No signatures, complete spends, nominal ABI inference, chain authentication, source authenticity or compiler preservation."}
    error = None
    try:
        modules, cases, curry = fixtures()
        run.materialize()
        report["generation"] = run.generate(modules)
        run.toolchain = tomllib.loads(read(ROOT / "rust-toolchain.toml").decode())["toolchain"]["channel"]
        cargo, native = run.native_and_extract()
        compiled = run.command("independent-chialisp-compile", [*cargo, "run", "--example", "compile-reference", *native], timeout=600)
        require(compiled == modules["multiply"].hex().encode() + b"\n", "independently compiled Chialisp bytes differ from original small module")
        report["independent_reference"] = {"source": file_identity(run.project / "examples/compile-reference.rs"), "artifact": identity(modules["multiply"])}
        report["source_body_mutation"] = run.source_mutation(cargo, native)
        run.extract()
        report["compiler_queries"] = {"driver_declaration": run.driver_compiler_declaration,
            "pinned_rustc_vV": run.compiler, "fields": run.compiler_fields,
            "scope": "Two actual retained commands; driver one-line build declaration cross-checked against independently queried pinned rustc -vV. Installed bytes are not authenticated."}
        report["bundles"], artifacts = run.inspect(modules)
        run.retained_before = run.retained_inputs()
        write_json(run.output / "retained-before.json", run.retained_before)
        report["canonical_outcomes"] = run.replay(modules, cases, artifacts, curry)
        report["actual_vm_observations"] = len(run.observations)
        report["accepted"] = sum(row["conclusive_outcome"]["status"] == "accepted" for row in run.observations)
        report["exact_typed_program_rejections"] = sum(row["conclusive_outcome"]["status"] == "rejected" for row in run.observations)
    except Exception as caught:
        error = caught
        report.update(status="inconclusive" if isinstance(caught, Inconclusive) else "failed", error=f"{type(caught).__name__}: {caught}")
    finally:
        # Always retain/check every actual batch input, including after child
        # launch, resource, normalization or comparison failure. Identity errors
        # are separate from policy observations and cannot earn rejection credit.
        vm_before = {"scope": "First actual bytes of each path, deduplicated; no authentication claim.", "files": run.vm_inputs.files()}
        vm_after = run.vm_inputs.snapshot()
        write_json(run.output / "vm-inputs-before.json", vm_before)
        write_json(run.output / "vm-inputs-after.json", vm_after)
        report["vm_input_inventory"] = {"before": file_identity(run.output / "vm-inputs-before.json"),
            "after": file_identity(run.output / "vm-inputs-after.json"),
            "batches": run.vm_batches, "final_status": vm_after["status"]}
        if vm_after["status"] != "matched":
            report["vm_input_inventory_error"] = "retained actual VM input inventory changed or is unreadable"
            if error is None:
                error = ValueError(report["vm_input_inventory_error"])
                report.update(status="failed", error=report["vm_input_inventory_error"])
        report["facade_feature_records"] = run.feature_records
        feature_after = run.feature_inputs.snapshot()
        report["facade_feature_inventory"] = {"before": run.feature_inputs.files(), "after": feature_after}
        if feature_after["status"] != "matched":
            report["facade_feature_inventory_error"] = "retained actual facade feature metadata changed or is unreadable"
            if error is None:
                error = ValueError(report["facade_feature_inventory_error"])
                report.update(status="failed", error=report["facade_feature_inventory_error"])
        if hasattr(run, "before"):
            try:
                after = run.quiet_inputs()
                write_json(run.output / "quiet-after.json", after)
                require(after == run.before, "compiler/source/generated/artifact input quiet window changed")
                report["quiet_window"] = "exact before/after source and tool identities checked"
                if hasattr(run, "retained_before"):
                    retained_after = run.retained_inputs()
                    write_json(run.output / "retained-after.json", retained_after)
                    require(retained_after == run.retained_before,
                            "retained complete bundles, exact IR/artifacts/source blobs or registry packages changed")
                    report["retained_quiet_window"] = "exact full retained bundle/store inventories checked before/after VM replay"
            except Exception as caught:
                report["quiet_window_error"] = f"{type(caught).__name__}: {caught}"
                if error is None:
                    error = caught
                    report.update(status="failed", error=report["quiet_window_error"])
        report["commands"], report["vm_observations"] = run.commands, run.observations
        report["resource_or_infrastructure_rejection_credit"] = 0
        if error is None:
            report["status"] = "passed"
        else:
            report["complete_workflow_credit"] = False
        write_json(run.output / "report.json", report)
    if error is not None:
        print(f"{report['status']}: {error}; partial report: {run.output / 'report.json'}", file=sys.stderr)
        return 1
    print(f"registry codegen: {len(run.commands)} actual child commands; {len(run.observations)} finite VM observations; no spend/signature/preservation credit")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
