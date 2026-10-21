#!/usr/bin/env python3
"""Exercise local exact pins and a real Rust host consumer of retrieved bytes.

Run after building the CLI/example and extracting complete corpus bundles.
This gate executes the already built tools only. It neither rebuilds a puzzle
nor authenticates its source, proves compiler preservation or submits spends.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

from cargo_source_receipt import decode_json, regular_path
from vm_failures import PROTOCOL as VM_PROTOCOL, check_failure

ENTRY = "chialisp_oxide_puzzles::capped_payment::puzzle"
SOLUTION = "ff8201f480"  # The proper one-element list (500).
EXPECTED = (
    "ffff49ff8203e880ffff33ffa0" + "42" * 32 + "ff8201f48080"
)  # ((ASSERT_MY_AMOUNT 1000) (CREATE_COIN fixed_recipient 500)).


def require(condition, message):
    if not condition:
        raise ValueError(message)


def manifest_path(root):
    """Resolve only the bounded, closed pointer from this requested build root."""
    root = root.resolve(strict=True)
    pointer_path = regular_path(root, "latest.json")
    with pointer_path.open("rb") as stream:
        raw = stream.read(4097)
    require(len(raw) <= 4096, "bundle pointer exceeds 4096 bytes")
    pointer = decode_json(raw)
    require(isinstance(pointer, dict) and set(pointer) == {"bundle", "manifest"},
            "unexpected bundle pointer shape")
    bundle = pointer["bundle"]
    require(isinstance(bundle, str) and bundle not in ("", ".", "..")
            and "/" not in bundle and "\\" not in bundle,
            "bundle pointer must name one directory")
    require(pointer["manifest"] == bundle + "/manifest.json",
            "bundle pointer/manifest mismatch")
    return regular_path(root, pointer["manifest"])


class Run:
    def __init__(self, args):
        self.cli = args.cli.resolve(strict=True)
        self.consumer = args.consumer.resolve(strict=True)
        self.output = args.output.resolve()
        self.output.mkdir(parents=True, exist_ok=False)
        self.commands = []
        self.environment = dict(os.environ, PATH="", CHIALISP_OXIDE_DRIVER="/nonexistent/driver",
                                RUSTC="/nonexistent/rustc", CARGO="/nonexistent/cargo",
                                RUSTC_WRAPPER="/nonexistent/wrapper",
                                RUSTC_WORKSPACE_WRAPPER="/nonexistent/workspace-wrapper")

    def command(self, binary, arguments, *, success=True):
        ordinal = len(self.commands)
        completed = subprocess.run([str(binary), *map(str, arguments)], cwd=self.output,
                                   env=self.environment, capture_output=True, timeout=60,
                                   check=False)
        label = f"{ordinal:02d}"
        (self.output / (label + ".stdout")).write_bytes(completed.stdout)
        (self.output / (label + ".stderr")).write_bytes(completed.stderr)
        self.commands.append({"binary": str(binary), "arguments": list(map(str, arguments)),
                              "exit_code": completed.returncode})
        require((completed.returncode == 0) == success,
                f"unexpected command result {label}: {completed.stderr[:1024]!r}")
        return decode_json(completed.stdout)

    def registry(self, *arguments, **kwargs):
        return self.command(self.cli, ["registry", *arguments, "--json"], **kwargs)

    def consume(self, lock, store, alias, solution=SOLUTION, **kwargs):
        return self.command(self.consumer, [lock, store, alias, solution], **kwargs)

    def execute(self, args):
        store = self.output / "original-registry"
        lock = self.output / "original.lock"
        packages = []
        for mode in ("off", "on"):
            source = manifest_path(getattr(args, "bundle_" + mode))
            with source.open("rb") as stream:
                manifest_bytes = stream.read(16 * 1024 * 1024 + 1)
            require(len(manifest_bytes) <= 16 * 1024 * 1024, "manifest exceeds byte bound")
            manifest = decode_json(manifest_bytes)
            entries = [row for row in manifest["entries"] if row["entry"] == ENTRY]
            require(len(entries) == 1, "bundle must contain exactly one capped payment export")
            artifact = entries[0]["artifact"]
            require(type(artifact["optimize_lir"]) is bool
                    and artifact["optimize_lir"] == (mode == "on"),
                    "bundle optimization mode differs from requested lane")
            published = self.registry("publish", "--bundle", source, "--entry", ENTRY,
                                      "--registry", store)
            package = published["package"]
            blob = bytes.fromhex(artifact["clvm_hex"])
            require(package["module_tree_hash"] == artifact["tree_hash"]
                    and package["blob_sha256"] == hashlib.sha256(blob).hexdigest(),
                    "published bytes differ from selected bundle artifact")
            added = self.registry("add", "--registry", store, "--lock", lock,
                                  "--name", "payment_" + mode, "--module-hash",
                                  package["module_tree_hash"], "--object-id", package["object_id"])
            require(added["status"] == "added", "new dependency was not added")
            result = self.consume(lock, store, "payment_" + mode)
            require(result["status"] == "executed" and result["result_hex"] == EXPECTED,
                    "retrieved payment did not produce independently specified conditions")
            require(result["vm_failure_protocol"] == VM_PROTOCOL,
                    "consumer VM protocol differs from pinned contract")
            require(result["object_id"] == package["object_id"]
                    and result["artifact"]["blob_sha256"] == package["blob_sha256"]
                    and result["artifact"]["module_tree_hash"] == package["module_tree_hash"],
                    "consumer did not identify the actual retrieved artifact")
            rejected = self.consume(lock, store, "payment_" + mode, "ff8203e980", success=False)
            check_failure(rejected["failure"])
            require(rejected["status"] == "vm_error"
                    and rejected["vm_failure_protocol"] == VM_PROTOCOL
                    and rejected["failure"] == {"kind": "program", "code": "raise"},
                    "over-cap execution must be the intended typed rejection")
            malformed = self.consume(lock, store, "payment_" + mode, "80", success=False)
            check_failure(malformed["failure"])
            require(malformed["status"] == "vm_error"
                    and malformed["vm_failure_protocol"] == VM_PROTOCOL
                    and malformed["failure"] == {"kind": "program", "code": "raise"},
                    "malformed ABI must reject with the expected typed code")
            packages.append({"mode": mode, "package": package, "execution": result,
                             "vm_failure_protocol": VM_PROTOCOL,
                             "over_cap": rejected["failure"], "malformed": malformed["failure"],
                             "bundle_manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest()})

        # Exact idempotence is stronger than re-encoding equivalent JSON.
        original = lock.read_bytes()
        decorated = b" \n" + original + b"\n "
        lock.write_bytes(decorated)
        selected = packages[0]["package"]
        repeated = self.registry("add", "--registry", store, "--lock", lock,
                                 "--name", "payment_off", "--module-hash",
                                 selected["module_tree_hash"], "--object-id", selected["object_id"])
        require(repeated["status"] == "already_present" and lock.read_bytes() == decorated,
                "idempotent add changed exact consumer bytes")
        wire = decode_json(decorated)
        require([pin["name"] for pin in wire["dependencies"]] == ["payment_off", "payment_on"]
                and all(pin["binding"] is None for pin in wire["dependencies"]),
                "lock entries must be sorted opaque pins")
        wire["dependencies"][0]["artifact"]["blob_sha256"] = "00" * 32
        lock.write_text(json.dumps(wire), encoding="utf-8")
        failed = self.consume(lock, store, "payment_on", success=False)
        require(failed["status"] == "host_error" and failed["kind"] == "digest_mismatch",
                "tampered pin must fail admission before VM execution")
        failed = self.registry("add", "--registry", store, "--lock", lock,
                               "--name", "other", "--module-hash", selected["module_tree_hash"],
                               "--object-id", selected["object_id"], success=False)
        require(failed["kind"] == "digest_mismatch", "tampered old pin was not rejected")
        lock.write_bytes(decorated)

        # Copy only packages and the portable lock; resolution cannot reopen the
        # original registry or require the consumer's source/bundle paths.
        copied_store = self.output / "copied-registry"
        copied_lock = self.output / "copied.lock"
        copied_store.mkdir()
        shutil.copytree(store / "objects", copied_store / "objects")
        copied_lock.write_bytes(decorated)
        shutil.rmtree(store)
        lock.unlink()
        for row in packages:
            result = self.consume(copied_lock, copied_store, "payment_" + row["mode"])
            require(result == row["execution"], "copied lock/store changed execution")

        target = copied_store / "objects" / (selected["object_id"] + ".json")
        retained = target.read_bytes()
        target.write_bytes(retained + b" ")
        failed = self.consume(copied_lock, copied_store, "payment_on", success=False)
        require(failed["status"] == "host_error" and failed["kind"] == "corrupt_object",
                "corrupted dependency must fail admission before VM execution")
        target.write_bytes(retained)
        target.unlink()
        failed = self.consume(copied_lock, copied_store, "payment_on", success=False)
        require(failed["status"] == "host_error" and failed["kind"] == "unknown_object",
                "missing dependency must fail admission before VM execution")
        target.write_bytes(retained)
        require(copied_lock.read_bytes() == decorated, "readonly failures changed the lock")
        return packages


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cli", "consumer", "bundle-on", "bundle-off", "output"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    run = Run(args)
    report = {"schema_version": 1, "scope": "offline_exact_pins_and_host_vm_only"}
    try:
        report["packages"] = run.execute(args)
        report["status"] = "passed"
    except Exception as error:
        report.update(status="failed", error=str(error))
        raise
    finally:
        report["commands"] = run.commands
        (run.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"registry dependencies: {len(run.commands)} offline child commands passed")


if __name__ == "__main__":
    main()
