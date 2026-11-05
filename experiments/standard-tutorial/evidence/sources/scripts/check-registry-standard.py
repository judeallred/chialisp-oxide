#!/usr/bin/env python3
"""Execute retrieved standard bindings with real source-produced inner puzzles.

Requires prebuilt tools and complete Cargo bundles in both optimization modes.
Only the separate Rust consumer executes a VM. This gate performs no compiler
build, signature validation, SDK spend, network request or chain authentication.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

from cargo_source_receipt import decode_json, regular_path
from vm_failures import PROTOCOL as VM_PROTOCOL, check_failure

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "crates/chialisp-oxide-package/tests/fixtures/mainnet-standard"
ENTRY = "chialisp_oxide_puzzles::capped_payment::puzzle"
ENCODING = "clvm-legacy-plain/v1"
MAX_BLOB = 2 * 1024 * 1024
MAX_JSON = 16 * 1024 * 1024
HEX = re.compile(r"(?:[0-9a-f]{2})+")
HASH = re.compile(r"[0-9a-f]{64}")
SOLUTION = "ff8201f480"  # Proper one-element list (500).
PAYMENT_CONDITIONS = bytes.fromhex(
    "ffff49ff8203e880ffff33ffa0" + "42" * 32 + "ff8201f48080"
)  # ((ASSERT_MY_AMOUNT 1000) (CREATE_COIN fixed_recipient 500)).
# Pinned chia-bls0.36.1 public_key.rs generator serialization control. This is
# a declared synthetic-key role, never a claim of derivation or authority.
SECOND_KEY = ("97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586"
              "c55e83ff97a1aeffb3af00adb22c6bb")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bounded_read(path, maximum):
    require(path.is_file() and not path.is_symlink(), f"not a regular input: {path}")
    require(path.stat().st_size <= maximum, f"input exceeds byte bound: {path}")
    with path.open("rb") as stream:
        value = stream.read(maximum + 1)
    require(len(value) <= maximum, f"input grew beyond byte bound: {path}")
    return value


def blob(raw):
    require(isinstance(raw, str) and len(raw) <= MAX_BLOB * 2 and HEX.fullmatch(raw),
            "artifact must be bounded nonempty lowercase hex")
    return bytes.fromhex(raw)


def plain_hash(data):
    """Independent iterative complete canonical-plain tree identity, no VM.

    This gate's fixed byte/node/depth caps bound work before atom hashing or
    stack growth. Product admission still runs separately in the actual CLI.
    """
    require(0 < len(data) <= MAX_BLOB, "plain hash byte bound")
    cursor, nodes, stack = 0, 0, []
    while True:
        require(cursor < len(data), "truncated plain tree")
        nodes += 1
        require(nodes <= 262_144 and len(stack) < 4096, "plain hash shape bound")
        head = data[cursor]
        cursor += 1
        if head == 0xff:
            stack.append(None)
            continue
        if head < 0x80:
            atom = memoryview(data)[cursor - 1:cursor]
        elif head == 0x80:
            atom = memoryview(data)[cursor:cursor]
        else:
            width = 0
            while head & (0x80 >> width):
                width += 1
            require(width <= 5, "unsupported plain atom/backreference prefix")
            require(cursor + width - 1 <= len(data), "truncated atom header")
            size = head & ((1 << (7 - width)) - 1)
            for byte in data[cursor:cursor + width - 1]:
                size = (size << 8) | byte
            cursor += width - 1
            require(size > 0 and cursor + size <= len(data), "truncated or padded atom")
            if width > 1:
                require(size >= 1 << (7 - (width - 1) + 8 * (width - 2)),
                        "nonminimal atom header")
            require(not (size == 1 and data[cursor] < 0x80), "noncanonical one-byte atom")
            atom = memoryview(data)[cursor:cursor + size]
            cursor += size
        hasher = hashlib.sha256(b"\x01")
        hasher.update(atom)
        value = hasher.digest()
        while stack and stack[-1] is not None:
            value = hashlib.sha256(b"\x02" + stack.pop() + value).digest()
        if stack:
            stack[-1] = value
        else:
            require(cursor == len(data), "trailing plain serialization")
            return value.hex()


def identity(data):
    return {"encoding": ENCODING, "bytes": len(data),
            "blob_sha256": hashlib.sha256(data).hexdigest(), "module_tree_hash": plain_hash(data)}


def canonical_reveal(module, key):
    require(len(key) == 48, "standard curry key width")
    # Exact reviewed (a (q . MODULE) (c (q . KEY) 1)) canonical curry syntax.
    return bytes.fromhex("ff02ffff01") + module + bytes.fromhex("ffff04ffff01b0") + key + bytes.fromhex("ff018080")


def delegated_solution(inner, solution):
    # Proper three-field list: nil original key, actual inner program, solution.
    return b"\xff\x80\xff" + inner + b"\xff" + solution + b"\x80"


def expected_conditions(key_hex, inner_hash):
    require(len(key_hex) == 96 and HASH.fullmatch(inner_hash), "condition identity shape")
    # Prepend (AGG_SIG_ME key sha256tree(inner_program)) to independently fixed
    # payment conditions. VM execution does not verify this signature condition.
    signature = bytes.fromhex("ff32ffb0" + key_hex + "ffa0" + inner_hash + "80")
    return (b"\xff" + signature + PAYMENT_CONDITIONS).hex()


def manifest_path(root):
    root = root.resolve(strict=True)
    raw = bounded_read(regular_path(root, "latest.json"), 4096)
    pointer = decode_json(raw)
    require(isinstance(pointer, dict) and set(pointer) == {"bundle", "manifest"},
            "unexpected bundle pointer shape")
    name = pointer["bundle"]
    require(isinstance(name, str) and name not in ("", ".", "..")
            and "/" not in name and "\\" not in name, "bundle pointer directory")
    require(pointer["manifest"] == name + "/manifest.json", "bundle pointer relation")
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
        command = {"ordinal": ordinal, "binary": str(binary),
                   "arguments": list(map(str, arguments)), "status": "starting"}
        self.commands.append(command)
        prefix = self.output / f"{ordinal:02d}"
        stdout, stderr = prefix.with_suffix(".stdout"), prefix.with_suffix(".stderr")
        try:
            with stdout.open("wb") as out, stderr.open("wb") as err:
                completed = subprocess.run([str(binary), *map(str, arguments)], cwd=self.output,
                                           env=self.environment, stdout=out, stderr=err,
                                           timeout=60, check=False)
            command.update(status="finished", exit_code=completed.returncode)
        except Exception as error:
            command.update(status="failed", error=str(error))
            raise
        raw = bounded_read(stdout, 8 * 1024 * 1024)
        diagnostic = bounded_read(stderr, 64 * 1024)
        command.update(stdout_sha256=hashlib.sha256(raw).hexdigest(),
                       stderr_sha256=hashlib.sha256(diagnostic).hexdigest())
        require((completed.returncode == 0) == success,
                f"unexpected command result {ordinal}: {diagnostic[:1024]!r}")
        return decode_json(raw)

    def registry(self, *arguments, **kwargs):
        return self.command(self.cli, ["registry", *arguments, "--json"], **kwargs)

    def consume(self, lock, store, standard, inner, key, solution=SOLUTION, **kwargs):
        return self.command(self.consumer, [lock, store, standard, inner, key, solution], **kwargs)

    def host_failure(self, lock, store, standard, inner, key, kind):
        failed = self.consume(lock, store, standard, inner, key, success=False)
        require(failed.get("status") == "host_error" and failed.get("kind") == kind,
                f"expected host {kind} before VM execution")
        require(not {"cost", "result_hex", "failure"} & failed.keys(),
                "host admission failure must not acquire VM execution evidence")
        return failed

    def publish_plain(self, store, name, data, provenance=None):
        package = {"schema_version": 1, "kind": "clvm_artifact",
                   "artifact": {"identity": identity(data), "clvm_hex": data.hex()},
                   "rust": None, "source_scope": "not_recorded", "sources": [],
                   "provenance": provenance}
        path = self.output / (name + ".json")
        path.write_text(json.dumps(package, indent=2) + "\n", encoding="utf-8")
        result = self.registry("publish", "--package", path, "--registry", store)["package"]
        require(result["object_id"] == hashlib.sha256(path.read_bytes()).hexdigest(),
                "published exact-envelope ID differs")
        require(result["module_tree_hash"] == package["artifact"]["identity"]["module_tree_hash"]
                and result["blob_sha256"] == package["artifact"]["identity"]["blob_sha256"],
                "published plain artifact identity differs")
        return result

    def add(self, store, lock, alias, package):
        result = self.registry("add", "--registry", store, "--lock", lock, "--name", alias,
                               "--module-hash", package["module_tree_hash"],
                               "--object-id", package["object_id"])
        require(result["status"] == "added" and result["pin"]["object_id"] == package["object_id"],
                "exact package dependency was not added")

    def success(self, result, standard, inner, key, module, inner_blob):
        reveal = canonical_reveal(module, bytes.fromhex(key))
        solution = delegated_solution(inner_blob, bytes.fromhex(SOLUTION))
        require(result["status"] == "executed" and result["scope"] == "host_standard_binding_vm_only"
                and result["vm_failure_protocol"] == VM_PROTOCOL, "consumer success scope/protocol")
        require(result["module_object_id"] == standard["object_id"]
                and result["inner_object_id"] == inner["object_id"]
                and result["synthetic_public_key_hex"] == key, "consumer retrieved object/key identity")
        require(result["reveal_hex"] == reveal.hex() and result["reveal"] == identity(reveal),
                "actual retrieved-module curry differs from independent exact wire")
        require(result["solution_hex"] == solution.hex() and result["solution"] == identity(solution),
                "outer binding does not preserve exact retrieved inner program/solution")
        require(type(result["cost"]) is int and 0 < result["cost"] <= 10_000_000,
                "successful VM cost missing or exceeds consumer bound")
        require(result["result_hex"] == expected_conditions(key, inner["module_tree_hash"]),
                "standard VM output differs from independent signature/payment conditions")
        require(result["reveal"]["module_tree_hash"] != standard["module_tree_hash"],
                "full curried puzzle hash must differ from standard module hash")

    def execute(self, args):
        fixture_raw = bounded_read(regular_path(FIXTURES, "expected.json"), 64 * 1024)
        fixture = decode_json(fixture_raw)
        module = blob(fixture["module_hex"])
        observed = blob(fixture["puzzle_reveal_hex"])
        key = fixture["arguments"][0]["hex"]
        require(len(module) == 227 and len(observed) == 291 and len(key) == 96,
                "retained standard fixture dimensions")
        require(identity(module)["module_tree_hash"] == fixture["module_tree_hash"]
                and identity(observed)["module_tree_hash"] == fixture["puzzle_tree_hash"]
                and canonical_reveal(module, bytes.fromhex(key)) == observed,
                "independent retained module/full identity and exact recurry")
        published_hex = bounded_read(regular_path(FIXTURES, "pinned-standard-module-hex.body"), 4096)
        require(bytes.fromhex(published_hex.decode().strip()) == module, "pinned published module bytes")
        reveal_path = self.output / "observed-reveal.hex"
        reveal_path.write_text(observed.hex() + "\n", encoding="utf-8")
        first_inspection = self.registry("inspect", "--clvm-hex-file", reveal_path,
                                         "--expected-puzzle-hash", fixture["puzzle_tree_hash"])
        inspected = first_inspection["inspection"]
        require(first_inspection["status"] == "inspected" and inspected["reveal"] == identity(observed)
                and inspected["module"]["identity"] == identity(module)
                and inspected["module"]["clvm_hex"] == module.hex()
                and inspected["standard_arguments"]["status"] == "decoded"
                and inspected["standard_arguments"]["synthetic_public_key_hex"] == key,
                "offline inspector disagrees with retained exact reveal")

        store, lock = self.output / "registry", self.output / "consumer.lock"
        standard = self.publish_plain(store, "standard-module", module, {
            "label": "Pinned canonical standard module from retained published hex",
            "source_url": "https://github.com/Chia-Network/chia_puzzles/blob/d122456feeef55708d354a065623037bfa010714/puzzles/p2_delegated_puzzle_or_hidden_puzzle.clsp",
            "revision": "d122456feeef55708d354a065623037bfa010714", "license": "Apache-2.0"})
        self.add(store, lock, "standard", standard)
        with_store = self.registry("inspect", "--clvm-hex-file", reveal_path, "--registry", store,
                                    "--expected-puzzle-hash", fixture["puzzle_tree_hash"])
        require(with_store["module_lookup"]["module_tree_hash"] == standard["module_tree_hash"]
                and [p["object_id"] for p in with_store["module_lookup"]["packages"]] == [standard["object_id"]],
                "inspection module lookup differs from exact local module package")
        full_lookup = self.registry("lookup", "--registry", store,
                                     "--module-hash", fixture["puzzle_tree_hash"])
        require(full_lookup["packages"] == [], "module-only package must not match full curried hash")

        packages, observations = [], []
        for mode in ("off", "on"):
            source = manifest_path(getattr(args, "bundle_" + mode))
            manifest_raw = bounded_read(source, MAX_JSON)
            manifest = decode_json(manifest_raw)
            selected = [row for row in manifest["entries"] if row["entry"] == ENTRY]
            require(len(selected) == 1, "bundle requires exactly one capped-payment export")
            artifact = selected[0]["artifact"]
            require(type(artifact["optimize_lir"]) is bool and artifact["optimize_lir"] == (mode == "on"),
                    "bundle optimization mode differs from selected lane")
            inner_blob = blob(artifact["clvm_hex"])
            inner_identity = identity(inner_blob)
            require(inner_identity["module_tree_hash"] == artifact["tree_hash"],
                    "bundle inner tree hash differs from independent plain-tree identity")
            # The CLI admits every immutable-bundle export before selecting one.
            inner = self.registry("publish", "--registry", store, "--bundle", source,
                                    "--entry", ENTRY)["package"]
            require(inner["module_tree_hash"] == inner_identity["module_tree_hash"]
                    and inner["blob_sha256"] == inner_identity["blob_sha256"], "source-produced inner identity")
            alias = "payment_" + mode
            self.add(store, lock, alias, inner)
            positive = self.consume(lock, store, "standard", alias, key)
            self.success(positive, standard, inner, key, module, inner_blob)
            require(positive["reveal_hex"] == observed.hex(), "fixture-key instance differs from observed reveal")
            negatives = []
            for solution in ("ff8203e980", "80"):
                failed = self.consume(lock, store, "standard", alias, key, solution, success=False)
                check_failure(failed.get("failure"))
                require(failed["status"] == "vm_error" and failed["vm_failure_protocol"] == VM_PROTOCOL
                        and failed["failure"] == {"kind": "program", "code": "raise"},
                        "over-cap/malformed inner environment must be the intended typed Raise")
                negatives.append({"solution_hex": solution, "failure": failed["failure"]})
            changed_key = self.consume(lock, store, "standard", alias, SECOND_KEY)
            self.success(changed_key, standard, inner, SECOND_KEY, module, inner_blob)
            require(changed_key["reveal"]["module_tree_hash"] != positive["reveal"]["module_tree_hash"],
                    "changing a key must change the full puzzle hash")
            packages.append(inner)
            observations.append({"mode": mode, "package": inner, "execution": positive,
                                 "changed_key_execution": changed_key, "typed_vm_negatives": negatives,
                                 "bundle_manifest_sha256": hashlib.sha256(manifest_raw).hexdigest()})

        host_controls = []
        for wrong_key in (key[:-2], "00" * 48, "c0" + "00" * 47):
            host_controls.append(self.host_failure(lock, store, "standard", "payment_on", wrong_key, "invalid_manifest"))
        host_controls.append(self.host_failure(lock, store, "payment_off", "payment_on", key, "digest_mismatch"))
        curried = self.publish_plain(store, "observed-curried-instance", observed)
        self.add(store, lock, "observed_instance", curried)
        host_controls.append(self.host_failure(lock, store, "observed_instance", "payment_on", key, "digest_mismatch"))
        full_lookup = self.registry("lookup", "--registry", store,
                                     "--module-hash", fixture["puzzle_tree_hash"])
        require([p["object_id"] for p in full_lookup["packages"]] == [curried["object_id"]],
                "full-hash lookup must return only the exact curried artifact package")

        original_lock = lock.read_bytes()
        wire = decode_json(original_lock)
        off_pin = next(pin for pin in wire["dependencies"] if pin["name"] == "payment_off")
        off_pin["artifact"]["blob_sha256"] = "00" * 32
        lock.write_text(json.dumps(wire), encoding="utf-8")
        host_controls.append(self.host_failure(lock, store, "standard", "payment_on", key, "digest_mismatch"))
        lock.write_bytes(original_lock)
        # Resolve every pin before VM use, including an unselected off-mode pin.
        target = store / "objects" / (packages[0]["object_id"] + ".json")
        original_package = bounded_read(target, MAX_JSON)
        target.write_bytes(original_package + b" ")
        host_controls.append(self.host_failure(lock, store, "standard", "payment_on", key, "corrupt_object"))
        target.write_bytes(original_package)
        target.unlink()
        host_controls.append(self.host_failure(lock, store, "standard", "payment_on", key, "unknown_object"))
        target.write_bytes(original_package)
        require(lock.read_bytes() == original_lock, "readonly host failures mutated the consumer lock")
        return {"fixture_sha256": hashlib.sha256(fixture_raw).hexdigest(),
                "standard_package": standard, "observed_reveal": identity(observed),
                "module_identity": identity(module), "packages": observations,
                "host_admission_controls": host_controls,
                "vm_failure_protocol": VM_PROTOCOL}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cli", "consumer", "bundle-on", "bundle-off", "output"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    run = Run(args)
    report = {"schema_version": 1, "scope": "offline_standard_binding_host_vm_only",
              "limitations": "No BLS signature validation, SDK ledger admission, complete spend, chain authentication, compiler preservation or production-readiness claim."}
    try:
        report["observations"] = run.execute(args)
        report["status"] = "passed"
    except Exception as error:
        report.update(status="failed", error=str(error))
        raise
    finally:
        report["commands"] = run.commands
        (run.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"registry standard: {len(run.commands)} offline child commands passed")


if __name__ == "__main__":
    main()
