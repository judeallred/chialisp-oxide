#!/usr/bin/env python3
"""Explicit prepare/native/capture phases for finite actual-source controls.

Cargo and driver execution require coordinated target ownership. Native Cargo
finishes before a separately reviewed LAST-host quiet receipt permits capture;
this script never manufactures that receipt. Receipts do not authenticate runs.
"""
from pathlib import Path
import argparse
import copy
import json
import os
import shutil
import subprocess
import tomllib

import boomerang_source_controls as c
from vm_failures import check_execution


def toolchain():
    return tomllib.loads(c.read(c.ROOT / "rust-toolchain.toml").decode())["toolchain"]["channel"]


def environment(target=None):
    value = dict(os.environ)
    for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_TARGET", "RUST_MIN_STACK", "RUSTC_BOOTSTRAP", "CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS"):
        value.pop(name, None)
    value.update(CARGO_NET_OFFLINE="true", RUSTUP_AUTO_INSTALL="0", RUSTUP_TOOLCHAIN=toolchain(), CARGO_BUILD_JOBS="2", PYTHONDONTWRITEBYTECODE="1")
    if target is not None:
        value.update(CARGO_TARGET_DIR=str(target), CHIALISP_OXIDE_DRIVER=str(target / "debug/chialisp-oxide-driver"))
    return value


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def query(argv, env):
    result = subprocess.run(list(map(str, argv)), cwd=c.ROOT, env=env, capture_output=True, timeout=180)
    c.require(type(result.returncode) is int and result.returncode == 0 and not result.stderr, "independent tool query failed/inconclusive")
    return result.stdout.decode()


def prepare(package, receipt):
    c.require(not package.exists() and not receipt.exists(), "new private package/receipt required")
    before = c.source_window()
    files, recipe = c.package_files(c.ROOT)
    package.mkdir(parents=True)
    for path, payload in files.items():
        destination = package / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(payload)
    prepared = dict(recipe=recipe, source_files=c.private_snapshot(package, locked=False)["sources"], witnesses=c.witnesses())
    c.require(c.source_window() == before, "source changed during package preparation")
    result = dict(schema_version=1, protocol="chialisp-oxide-boomerang-controls-prepare/v1", status="exact_source_inputs_pinned_before_first_Cargo",
        source_root=str(c.ROOT), package_root=str(package), toolchain=toolchain(), source_window=before, prepared=prepared)
    write(receipt, result)
    return result


def native(package, prepared_path, target, output):
    c.require(not output.exists(), "new native receipt/log directory required")
    output.mkdir(parents=True)
    prepared = c.load(prepared_path)
    c.closed(prepared, ["schema_version", "protocol", "status", "source_root", "package_root", "toolchain", "source_window", "prepared"], "prepare receipt")
    c.require(type(prepared["schema_version"]) is int and prepared["schema_version"] == 1
        and prepared["protocol"] == "chialisp-oxide-boomerang-controls-prepare/v1"
        and prepared["status"] == "exact_source_inputs_pinned_before_first_Cargo"
        and prepared["source_root"] == str(c.ROOT) and prepared["package_root"] == str(package)
        and prepared["toolchain"] == toolchain() and prepared["source_window"] == c.source_window(), "prepared source identity")
    files, recipe = c.package_files(c.ROOT)
    c.require(prepared["prepared"] == dict(recipe=recipe, source_files=c.private_snapshot(package, locked=False)["sources"], witnesses=c.witnesses()), "exact prepared package/recipe identities")
    fixed = {path: c.digest(payload) for path, payload in files.items()}
    def unchanged():
        c.require(c.source_window() == prepared["source_window"] and all(c.sha(package / path) == pin for path, pin in fixed.items()), "source changed during native Cargo phase")
    phases = [
        (["generate-lockfile", "--offline"], []),
        (["fmt"], ["--all", "--check"]),
        (["check"], ["--all-targets", "--locked", "--offline"]),
        (["test"], ["--locked", "--offline", "--", "--nocapture"]),
        (["clippy"], ["--all-targets", "--locked", "--offline", "--", "-D", "warnings"]),
        (["build"], ["--example", "ledger", "--locked", "--offline"]),
    ]
    commands = []
    for index, (flags, tail) in enumerate(phases):
        unchanged()
        argv = ["rustup", "run", toolchain(), "cargo", *flags, "--manifest-path", str(package / "Cargo.toml"), *tail]
        log = output / f"{index}.log"
        with log.open("wb") as stream:
            process = subprocess.run(argv, cwd=c.ROOT, env=environment(target), stdout=stream, stderr=subprocess.STDOUT, timeout=1800)
        commands.append(dict(argv=argv, exit_code=process.returncode, log=f"native-logs/{index}.log", log_sha256=c.sha(log)))
        c.require(type(process.returncode) is int and process.returncode == 0, "native Cargo/format phase failed; no rejection or test credit")
        if index == 0:
            fixed["Cargo.lock"] = c.sha(package / "Cargo.lock")
        unchanged()
    native_log = output / "3.log"
    c.native_rows(c.read(native_log).decode())
    shutil.copy2(native_log, output / "native.log")
    binary = target / "debug/examples/ledger"
    snapshot = c.private_snapshot(package)
    metadata = copy.deepcopy(prepared["prepared"])
    metadata["source_files"] = snapshot["sources"]
    result = dict(schema_version=1, protocol="chialisp-oxide-boomerang-controls-native/v1", status="native_phase_passed_before_LAST_host_build",
        source_root=str(c.ROOT), package_root=str(package), toolchain=toolchain(), source_window=prepared["source_window"], prepared=metadata,
        native=dict(argv=commands[3]["argv"], exit_code=0, native_test_groups=1, native_policy_observations=9,
            log="native.log", log_sha256=c.sha(native_log), ledger_binary=dict(path=str(binary), sha256=c.sha(binary))), commands=commands)
    write(output / "native-receipt.json", result)
    return result


def capture(package, native_path, quiet_path, target, output):
    c.require(not output.exists(), "new capture output required")
    receipt, quiet = c.load(native_path), c.load(quiet_path)
    c.closed(quiet, ["schema_version", "protocol", "status", "source_root", "package_root", "compiler", "toolchain", "source_window", "tools", "native_receipt_sha256", "scope"], "LAST-host quiet receipt")
    c.closed(receipt, ["schema_version", "protocol", "status", "source_root", "package_root", "toolchain", "source_window", "prepared", "native", "commands"], "native phase receipt")
    env = environment(target)
    compiler = query(["rustup", "run", toolchain(), "rustc", "-vV"], env)
    c.require(query([target / "debug/chialisp-oxide-driver", "--compiler-version"], env).strip() == compiler.splitlines()[0], "independent driver/compiler versions differ")
    tools = {name: dict(path=str(target / "debug" / name), sha256=c.sha(target / "debug" / name)) for name in c.TOOLS}
    c.require(type(quiet["schema_version"]) is int and quiet["schema_version"] == 1
        and quiet["protocol"] == "chialisp-oxide-boomerang-controls-quiet-window/v1"
        and quiet["status"] == "LAST_host_build_terminal_tools_quiet"
        and quiet["source_root"] == str(c.ROOT) and quiet["package_root"] == str(package)
        and quiet["compiler"] == compiler and quiet["toolchain"] == toolchain() and quiet["tools"] == tools
        and quiet["source_window"] == c.source_window() and quiet["native_receipt_sha256"] == c.sha(native_path)
        and type(quiet["scope"]) is str and quiet["scope"], "separately reviewed LAST-host quiet source/tool/native receipt required")
    c.require(type(receipt["schema_version"]) is int and receipt["schema_version"] == 1
        and receipt["protocol"] == "chialisp-oxide-boomerang-controls-native/v1"
        and receipt["status"] == "native_phase_passed_before_LAST_host_build"
        and all(receipt[key] == quiet[key] for key in ("source_root", "package_root", "toolchain", "source_window")), "native/LAST-host input linkage")
    files, recipe = c.package_files(c.ROOT)
    snapshot = c.private_snapshot(package)
    c.require(receipt["prepared"] == dict(recipe=recipe, source_files=snapshot["sources"], witnesses=c.witnesses()), "current/native exact source package identity")
    native = receipt["native"]
    c.require(native["native_test_groups"] == 1 and type(native["native_test_groups"]) is int
        and native["native_policy_observations"] == 9 and type(native["native_policy_observations"]) is int
        and native["log"] == "native.log" and c.sha(native_path.parent / "native.log") == native["log_sha256"], "native group/output/log linkage")
    c.native_rows(c.read(native_path.parent / "native.log").decode())
    binary = c.absolute(native["ledger_binary"]["path"])
    c.require(c.sha(binary) == native["ledger_binary"]["sha256"], "native SDK runner changed")
    fixed = {str(package / row["path"]): row["sha256"] for row in snapshot["sources"]}
    fixed.update({str(native_path): c.sha(native_path), str(quiet_path): c.sha(quiet_path), str(binary): c.sha(binary)})
    fixed.update({row["path"]: row["sha256"] for row in tools.values()})
    def unchanged():
        c.require(c.source_window() == quiet["source_window"] and all(c.sha(Path(path)) == pin for path, pin in fixed.items()), "source/package/tool/receipt changed during capture")
    unchanged()
    output.mkdir(parents=True)
    for path in c.FILES + ["Cargo.lock"]:
        destination = output / "package" / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(package / path, destination)
    for source, name in [(native_path, "native-receipt.json"), (native_path.parent / "native.log", "native.log"), (quiet_path, "last-host-window.json")]:
        shutil.copy2(source, output / name)
    (output / "native-logs").mkdir()
    for index, command in enumerate(receipt["commands"]):
        source = native_path.parent / f"{index}.log"
        c.require(c.sha(source) == command["log_sha256"], "native command log changed")
        shutil.copy2(source, output / command["log"])
    for path in output.rglob("*"):
        if path.is_file():
            fixed[str(path)] = c.sha(path)
    commands = []
    (output / "logs").mkdir()
    def execute(role, argv):
        unchanged()
        index = len(commands)
        stdout, stderr = output / f"logs/{index:02}.stdout", output / f"logs/{index:02}.stderr"
        with stdout.open("wb") as out, stderr.open("wb") as err:
            process = subprocess.run(list(map(str, argv)), cwd=c.ROOT, env=env, stdout=out, stderr=err, timeout=1800)
        commands.append(dict(role=role, argv=list(map(str, argv)), exit_code=process.returncode,
            stdout=stdout.relative_to(output).as_posix(), stdout_sha256=c.sha(stdout),
            stderr=stderr.relative_to(output).as_posix(), stderr_sha256=c.sha(stderr)))
        unchanged()
        c.require(type(process.returncode) is int and process.returncode == 0
            and (role.startswith("export-") or not c.read(stderr, allow_empty=True)), "capture command failed/inconclusive; no policy rejection credit")
        fixed[str(stdout)], fixed[str(stderr)] = c.sha(stdout), c.sha(stderr)
        return c.read(stdout, allow_empty=True)
    bundles, instances, runs, ledger_inputs = {}, {}, [], []
    for mode in ("off", "on"):
        directory = output / f"fresh-{mode}"
        execute("export-" + mode, [target / "debug/cargo-chialisp-oxide", "build", *c.options(package), "--output-dir", directory] + (["--no-optimize"] if mode == "off" else []))
        checked = c.fresh_bundle(directory, package, compiler=compiler,
            driver_sha256=tools["chialisp-oxide-driver"]["sha256"], toolchain=toolchain(), optimize=mode == "on")
        retained = output / "bundles" / mode
        retained.mkdir(parents=True)
        shutil.copy2(checked.manifest_path, retained / "manifest.json")
        by_name, recompiled = {}, []
        instances[mode] = {}
        for entry in checked.manifest["entries"]:
            name = entry["entry"].split("::")[-2]
            for key in ("ir", "clvm"):
                shutil.copy2(checked.manifest_path.parent / entry[key], retained / entry[key])
                fixed[str(retained / entry[key])] = c.sha(retained / entry[key])
            ir = retained / entry["ir"]
            c.strict_abi(c.load(ir), entry["entry"])
            artifacts = json.loads(execute(f"recompile-{mode}-{name}", [target / "debug/chialisp-oxide-verifier", ir, "--artifacts"]), object_pairs_hook=c.cargo_authoring.unique_object)
            selected = next(artifact for artifact in artifacts if artifact["optimize"] is (mode == "on"))
            c.require(all(selected[key] == entry["artifact"][key] for key in ("bytes", "tree_hash", "clvm_hex")), "validated IR recompilation differs from Cargo artifact")
            recompiled.append(dict(entry=entry["entry"], ir_sha256=c.sha(ir), artifacts=artifacts))
            by_name[name] = entry
            instance = c.curry(bytes.fromhex(entry["artifact"]["clvm_hex"]))
            instance_path = output / "instances" / mode / (name + ".clvm.hex")
            instance_path.parent.mkdir(parents=True, exist_ok=True)
            instance_path.write_text(instance.hex() + "\n")
            instances[mode][name] = dict(path=instance_path.relative_to(output).as_posix(), bytes=len(instance), clvm_hex=instance.hex(),
                tree_hash=c.raw_tree.tree_hash(c.raw_tree.deserialize(instance)).hex(), module_tree_hash=entry["artifact"]["tree_hash"])
            fixed[str(instance_path)] = c.sha(instance_path)
        fixed[str(retained / "manifest.json")] = c.sha(retained / "manifest.json")
        bundles[mode] = dict(manifest=f"bundles/{mode}/manifest.json", manifest_sha256=c.sha(retained / "manifest.json"), recompiled=recompiled)
        for spec in c.witnesses():
            name = spec["name"]
            solution_path = output / "inputs" / f"{mode}-{name}-{spec['witness']}.json"
            solution_path.parent.mkdir(parents=True, exist_ok=True)
            write(solution_path, [spec["instance_solution"]])
            fixed[str(solution_path)] = c.sha(solution_path)
            instance = instances[mode][name]
            result = json.loads(execute(f"vm-{mode}-{name}-{spec['witness']}", [target / "debug/chialisp-oxide", "run-batch", output / instance["path"], "--solutions", solution_path, "--max-cost", "1000000"]))
            c.require(type(result) is list and len(result) == 1 and check_execution(result[0]) == spec["expected"], "retained curried VM output differs from independent source witness")
            runs.append(dict(mode=mode, name=name, witness=spec["witness"], solution=spec["instance_solution"],
                instance_tree_hash=instance["tree_hash"], instance_sha256=c.digest(bytes.fromhex(instance["clvm_hex"])), execution=result[0]))
            ledger_inputs.append(dict(mode=mode, name=name, witness=spec["witness"], module_hex=by_name[name]["artifact"]["clvm_hex"], instance_hex=instance["clvm_hex"]))
    request = output / "ledger-request.json"
    write(request, dict(protocol=c.LEDGER_PROTOCOL, runs=ledger_inputs))
    fixed[str(request)] = c.sha(request)
    ledger_bytes = execute("sdk-ledger", [binary, request])
    ledger_result = json.loads(ledger_bytes, object_pairs_hook=c.cargo_authoring.unique_object)
    c.require(ledger_result["protocol"] == c.LEDGER_PROTOCOL and ledger_result["transaction_attempts"] == 18 and len(ledger_result["attempts"]) == 18, "eighteen actual SDK submissions required")
    (output / "ledger.json").write_bytes(ledger_bytes)
    ledger = dict(request="ledger-request.json", request_sha256=c.sha(request), output="ledger.json", output_sha256=c.sha(output / "ledger.json"), result=ledger_result)
    context = dict(source_root=str(c.ROOT), package_root=str(package), capture_root=str(output), compiler=compiler, toolchain=toolchain(),
        source_window=quiet["source_window"], tools=tools, last_host_window=dict(path=str(quiet_path), sha256=c.sha(quiet_path)))
    inventory = set("package/" + path for path in c.FILES + ["Cargo.lock"])
    inventory.update(["native.log", "native-receipt.json", "last-host-window.json", "ledger-request.json", "ledger.json"])
    inventory.update(command["log"] for command in receipt["commands"])
    inventory.update(command[key] for command in commands for key in ("stdout", "stderr"))
    inventory.update(path.relative_to(output).as_posix() for path in (output / "inputs").glob("*.json"))
    for mode, bundle in bundles.items():
        inventory.add(bundle["manifest"])
        manifest = c.load(output / bundle["manifest"])
        inventory.update(f"bundles/{mode}/" + entry[key] for entry in manifest["entries"] for key in ("ir", "clvm"))
        inventory.update(instance["path"] for instance in instances[mode].values())
    report = dict(schema_version=1, protocol=c.PROTOCOL, status="actual_source_controls_captured", scope=c.SCOPE,
        context=context, prepared=receipt["prepared"], native=native, bundles=bundles, instances=instances,
        runs=runs, ledger=ledger, commands=commands, files=[dict(path=path, sha256=c.sha(output / path)) for path in sorted(inventory)])
    unchanged()
    write(output / "report.json", report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    phases = parser.add_subparsers(dest="phase", required=True)
    prepare_parser = phases.add_parser("prepare")
    prepare_parser.add_argument("--package", type=Path, required=True)
    prepare_parser.add_argument("--receipt", type=Path, required=True)
    native_parser = phases.add_parser("native")
    native_parser.add_argument("--package", type=Path, required=True)
    native_parser.add_argument("--prepared", type=Path, required=True)
    native_parser.add_argument("--target", type=Path, required=True)
    native_parser.add_argument("--output", type=Path, required=True)
    capture_parser = phases.add_parser("capture")
    capture_parser.add_argument("--package", type=Path, required=True)
    capture_parser.add_argument("--native-receipt", type=Path, required=True)
    capture_parser.add_argument("--last-host-window", type=Path, required=True)
    capture_parser.add_argument("--target", type=Path, required=True)
    capture_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.phase == "prepare":
            result = prepare(args.package.resolve(), args.receipt.resolve())
        elif args.phase == "native":
            result = native(args.package.resolve(), args.prepared.resolve(), args.target.resolve(), args.output.resolve())
        else:
            result = capture(args.package.resolve(), args.native_receipt.resolve(), args.last_host_window.resolve(), args.target.resolve(), args.output.resolve())
    except BaseException as error:
        if hasattr(args, "output") and args.output.exists():
            failure = args.output / "incomplete-failure.json"
            if not failure.exists():
                write(failure, dict(status="incomplete_failure_no_success_credit", phase=args.phase,
                    error_type=type(error).__name__, message=str(error),
                    scope="No automatic retry/reuse. Inspect owned children and late writes after timeouts. Resource/infrastructure failures cannot prove policy rejection. Existing logs retained."))
        raise
    print(json.dumps(dict(status=result["status"], scope=__doc__), indent=2))


if __name__ == "__main__":
    main()
