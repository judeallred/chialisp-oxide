#!/usr/bin/env python3
"""Materialize the ordinary Cargo example and exact offline dependency recipe.

Uses only a prebuilt registry CLI. No Cargo/compiler/VM/network execution.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parents[2]
TEMPLATE = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "scripts"))
from registry_codegen_evidence import digest, file_identity, identity, read, require, write_json

spec = importlib.util.spec_from_file_location("registry_process_gate", ROOT / "scripts/check-registry-codegen.py")
process_gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(process_gate)

ENTRY = "registry_standard_wallet::standard_wallet"
MODULE_SHA = "21747ef070376012f22f7b426edaa36a244a77f8825d13d5529963b921ad1da7"
MODULE_HASH = "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52"


class Commands(process_gate.Run):
    # Reuse the reviewed bounded child-output/log transport, not its producer.
    def __init__(self, output, cli):
        self.output = Path(output)
        self.output.mkdir(parents=True, exist_ok=False)
        self.commands = []
        self.environment = dict(os.environ, CARGO_NET_OFFLINE="true")
        self.offline = dict(self.environment, PATH="", CARGO="/nonexistent/cargo", RUSTC="/nonexistent/rustc",
                            CHIALISP_OXIDE_DRIVER="/nonexistent/driver", RUSTC_WRAPPER="/nonexistent/wrapper",
                            RUSTC_WORKSPACE_WRAPPER="/nonexistent/workspace-wrapper")
        self.cli = Path(cli).resolve(strict=True)

    def registry(self, label, *args, expected=0):
        return self.command(label, [self.cli, "registry", *args, "--json"], offline=True,
                            expected=expected, json_result=True)


def prepare(output, cli):
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    commands = Commands(output / "prepare-commands", cli)
    fixture = ROOT / "crates/chialisp-oxide-package/tests/fixtures/mainnet-standard"
    provenance = json.loads(read(fixture / "provenance.json"))
    require({p.name for p in fixture.iterdir()} == {row["path"] for row in provenance["files"]} | {"provenance.json"},
            "closed retained standard fixture inventory")
    for row in provenance["files"]:
        raw = read(fixture / row["path"], 65536)
        require(len(raw) == row["bytes"] and digest(raw) == row["sha256"], "retained standard fixture identity")
    expected = json.loads(read(fixture / "expected.json"))
    module = bytes.fromhex(expected["module_hex"])
    require(len(module) == 227 and digest(module) == MODULE_SHA and identity(module)["module_tree_hash"] == MODULE_HASH,
            "reviewed standard module identity")
    project = output / "project"
    (project / "src/generated").mkdir(parents=True)
    (project / "examples").mkdir()
    for name in ("src/lib.rs", "examples/ledger.rs", "README.md"):
        shutil.copyfile(TEMPLATE / name, project / name)
    manifest = read(TEMPLATE / "Cargo.toml").decode()
    for crate in ("chialisp-oxide", "chialisp-oxide-simulator"):
        old = f'"../../crates/{crate}"'
        require(manifest.count(old) == (2 if crate == "chialisp-oxide" else 1), "explicit manifest rewrite")
        manifest = manifest.replace(old, json.dumps(str(ROOT / "crates" / crate)))
    old = '"../../experiments/backend"'
    require(manifest.count(old) == 1, "explicit backend manifest rewrite")
    manifest = manifest.replace(old, json.dumps(str(ROOT / "experiments/backend")))
    (project / "Cargo.toml").write_text(manifest)
    package = {"schema_version": 1, "kind": "clvm_artifact",
               "artifact": {"identity": identity(module), "clvm_hex": module.hex()},
               "rust": None, "source_scope": "not_recorded", "sources": [], "provenance": None}
    package_path = output / "standard.package.json"
    write_json(package_path, package)
    store, lock = output / "registry", project / "dependencies.lock"
    published = commands.registry("publish", "publish", "--package", package_path, "--registry", store)["package"]
    require(published["object_id"] == digest(read(package_path)) and published["module_tree_hash"] == MODULE_HASH
            and published["blob_sha256"] == MODULE_SHA, "exact package/envelope/blob/module identities")
    added = commands.registry("pin", "add", "--registry", store, "--lock", lock, "--name", "standard",
                              "--module-hash", MODULE_HASH, "--object-id", published["object_id"])
    require(added["pin"]["binding"] is None, "opaque dependency: no inferred nominal binding")
    source = project / "src/generated/standard.rs"
    args = ("generate-rust", "--registry", store, "--lock", lock, "--name", "standard", "--output", source)
    generation = commands.registry("generate", *args)
    require(generation["status"] == "created", "fresh exact generated source")
    exact_source = read(source, 262144)
    exact_lock = read(lock)
    for label, mutate in (("missing-dependency", "missing"), ("corrupt-dependency", "corrupt"), ("edited-generated", "source")):
        object_path = store / "objects" / (published["object_id"] + ".json")
        object_bytes = read(object_path)
        try:
            if mutate == "missing": object_path.unlink()
            elif mutate == "corrupt": object_path.write_bytes(b"{}\n")
            else: source.write_bytes(exact_source + b"\n// edited generated control\n")
            before_source = read(source, 262144)
            failed = commands.registry(label, *args, "--check", expected=1)
            require(failed["status"] == "error" and read(source, 262144) == before_source
                    and read(lock) == exact_lock, "offline negative must fail without mutation")
            if mutate == "source": require(failed["kind"] == "conflict", "edited-source exact conflict")
        finally:
            object_path.write_bytes(object_bytes)
            source.write_bytes(exact_source)
    checked = commands.registry("exact-regeneration-check", *args, "--check")
    require(checked["status"] == "checked", "restored exact dependency recipe")
    write_json(output / "prepared.json", {"protocol": "chialisp-oxide-registry-standard-wallet-prepared/v1",
        "status": "offline_prepared_not_executed", "source_root": str(ROOT), "project": str(project),
        "registry": str(store), "lock": file_identity(lock), "module": identity(module),
        "package": published, "generation": generation["generation"], "generated_source": file_identity(source),
        "project_sources": [file_identity(project / name) for name in
            ("Cargo.toml", "src/lib.rs", "examples/ledger.rs", "README.md", "dependencies.lock", "src/generated/standard.rs")],
        "template_sources": [file_identity(TEMPLATE / name) for name in
            ("Cargo.toml", "src/lib.rs", "examples/ledger.rs", "prepare.py", "check.py", "README.md")],
        "fixture_provenance": file_identity(fixture / "provenance.json"),
        "scope": "Offline exact envelope/lock/generation and constructed failure controls only; no native, compiler, VM or SDK execution."})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry-cli", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    prepare(options.output, options.registry_cli)
