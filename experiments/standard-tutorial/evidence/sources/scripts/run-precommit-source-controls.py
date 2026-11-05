#!/usr/bin/env python3
"""Prepare/native/capture actual precommit mutations in coordinated phases.

Run native Cargo before the coordinator's LAST host build and quiet receipt.
Capture executes prebuilt tools, paired Cargo extraction and actual SDK spends.
Failed runs are preserved; no failure earns rejection credit.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

import precommit_source_controls as c


def toolchain():
    return tomllib.loads(c.read(c.ROOT / "rust-toolchain.toml").decode())["toolchain"]["channel"]


def environment(target=None):
    env = dict(os.environ)
    for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
                 "CARGO_BUILD_TARGET", "RUST_MIN_STACK", "RUSTC_BOOTSTRAP", "CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS"):
        env.pop(name, None)
    env.update(CARGO_NET_OFFLINE="true", RUSTUP_AUTO_INSTALL="0", RUSTUP_TOOLCHAIN=toolchain(),
               CARGO_BUILD_JOBS="2", PYTHONDONTWRITEBYTECODE="1")
    if target is not None:
        env.update(CARGO_TARGET_DIR=str(target), CHIALISP_OXIDE_DRIVER=str(target / "debug/chialisp-oxide-driver"))
    return env


def query(argv):
    run = subprocess.run(list(map(str, argv)), cwd=c.ROOT, env=environment(), capture_output=True, timeout=60)
    c.require(run.returncode == 0 and not run.stderr, "tool identity query failed/inconclusive")
    return run.stdout.decode()


def prepare(package, receipt):
    c.require(not package.exists() and not receipt.exists(), "new package/prepare receipt required")
    window = c.source_window()
    files = c.package_files()
    for path, payload in files.items():
        destination = package / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(payload)
    c.require(c.source_window() == window, "sources changed during prepare")
    c.write(receipt, dict(protocol="chialisp-oxide-precommit-controls-prepare/v1", source_root=str(c.ROOT),
                          package_root=str(package), source_window=window,
                          files=[dict(path=path, sha256=c.digest(payload)) for path, payload in sorted(files.items())]))


def native(package, prepared, target, output):
    c.require(not output.exists(), "new native output required")
    receipt = c.load(prepared)
    c.closed(receipt, ("protocol", "source_root", "package_root", "source_window", "files"), "prepare receipt")
    window = c.source_window()
    files = c.package_files()
    c.require(receipt == dict(protocol="chialisp-oxide-precommit-controls-prepare/v1", source_root=str(c.ROOT),
                              package_root=str(package), source_window=window,
                              files=[dict(path=path, sha256=c.digest(payload)) for path, payload in sorted(files.items())]),
              "exact prepared source window")
    def unchanged():
        c.require(c.source_window() == window and all(c.read(package / path) == payload for path, payload in files.items()),
                  "sources changed during native phase")
    output.mkdir(parents=True)
    commands = []
    phases = (("generate-lockfile", ["--offline"]), ("check", ["--all-targets", "--locked", "--offline"]),
              ("test", ["--locked", "--offline", "--", "--nocapture"]),
              ("clippy", ["--all-targets", "--locked", "--offline", "--", "-D", "warnings"]))
    for index, (operation, tail) in enumerate(phases):
        unchanged()
        argv = ["rustup", "run", toolchain(), "cargo", operation, "--manifest-path", str(package / "Cargo.toml"), *tail]
        log = output / f"{index}.log"
        with log.open("wb") as stream:
            run = subprocess.run(argv, cwd=c.ROOT, env=environment(target), stdout=stream,
                                 stderr=subprocess.STDOUT, timeout=1800)
        commands.append(dict(argv=argv, exit_code=run.returncode, log=f"native-logs/{index}.log", log_sha256=c.sha(log)))
        c.require(run.returncode == 0, "native Cargo phase failed; no rejection credit")
        unchanged()
    shutil.copy2(output / "2.log", output / "native.log")
    c.native_rows(c.read(output / "native.log").decode())
    c.write(output / "native-receipt.json", dict(schema_version=1, protocol="chialisp-oxide-precommit-controls-native/v1",
        status="native_phase_passed_before_LAST_host_build", source_root=str(c.ROOT), package_root=str(package),
        toolchain=toolchain(), source_window=window, prepared=c.private_snapshot(package), commands=commands))


def capture(package, native_path, quiet_path, target, output):
    work = output.parent / (output.name + "-work")
    c.require(not output.exists() and not work.exists(), "new capture/work outputs required")
    native = c.load(native_path)
    quiet = c.load(quiet_path)
    compiler = query(["rustup", "run", toolchain(), "rustc", "-vV"])
    c.require(query([target / "debug/chialisp-oxide-driver", "--compiler-version"]).strip() == compiler.splitlines()[0],
              "independently queried driver/compiler identity")
    tools = {name: dict(path=str(target / "debug" / name), sha256=c.sha(target / "debug" / name)) for name in c.TOOLS}
    ledger_runner = dict(path=str(target / "debug/examples/precommit_controls"), sha256=c.sha(target / "debug/examples/precommit_controls"))
    window = c.source_window()
    c.require(native["protocol"] == "chialisp-oxide-precommit-controls-native/v1"
              and native["status"] == "native_phase_passed_before_LAST_host_build"
              and native["source_root"] == str(c.ROOT) and native["package_root"] == str(package)
              and native["source_window"] == window and native["prepared"] == c.private_snapshot(package), "native source linkage")
    c.require(quiet["protocol"] == "chialisp-oxide-precommit-controls-quiet-window/v1"
              and quiet["status"] == "LAST_host_build_terminal_tools_quiet" and quiet["source_root"] == str(c.ROOT)
              and quiet["package_root"] == str(package) and quiet["compiler"] == compiler
              and quiet["toolchain"] == toolchain() and quiet["tools"] == tools
              and quiet["source_window"] == window and quiet["native_receipt_sha256"] == c.sha(native_path)
              and quiet["ledger_runner"] == ledger_runner, "LAST host quiet window")
    def unchanged():
        c.require(c.source_window() == window and c.private_snapshot(package) == native["prepared"]
                  and all(c.sha(Path(row["path"])) == row["sha256"] for row in [*tools.values(), ledger_runner]),
                  "sources/package/host tools changed during capture")
    unchanged()
    output.mkdir(parents=True)
    work.mkdir()
    for row in native["prepared"]["sources"]:
        destination = output / "package" / row["path"]
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(package / row["path"], destination)
    for source, name in ((native_path, "native-receipt.json"), (quiet_path, "last-host-window.json"),
                         (native_path.parent / "native.log", "native.log")):
        shutil.copy2(source, output / name)
    (output / "native-logs").mkdir()
    for index, command in enumerate(native["commands"]):
        c.require(command["exit_code"] == 0 and c.sha(native_path.parent / f"{index}.log") == command["log_sha256"], "native command/log")
        shutil.copy2(native_path.parent / f"{index}.log", output / command["log"])
    (output / "logs").mkdir()
    commands = []
    def execute(role, argv):
        unchanged()
        index = len(commands)
        out, err = output / f"logs/{index:02}.stdout", output / f"logs/{index:02}.stderr"
        with out.open("wb") as stdout, err.open("wb") as stderr:
            result = subprocess.run(list(map(str, argv)), cwd=c.ROOT, env=environment(target),
                                    stdout=stdout, stderr=stderr, timeout=1800)
        commands.append(dict(role=role, argv=list(map(str, argv)), exit_code=result.returncode,
                             stdout=out.relative_to(output).as_posix(), stderr=err.relative_to(output).as_posix()))
        c.require(result.returncode == 0 and (role.startswith("export-") or not c.read(err, allow_empty=True)),
                  "capture command failed/inconclusive")
        unchanged()
        return c.read(out)
    bundles, runs, ledger_runs = {}, [], []
    for mode in ("off", "on"):
        fresh = work / f"fresh-{mode}"
        execute("export-" + mode, [target / "debug/cargo-chialisp-oxide", "build", *c.options(package),
                                   "--output-dir", fresh, *(["--no-optimize"] if mode == "off" else [])])
        private = c.private_snapshot(package)
        checked = None
        for name in c.NAMES:
            loaded = c.cargo_bundle.load_declared_export(fresh, root=package, authoring=private,
                selected_entry=private["exports"][name]["entry"], compiler=compiler,
                driver_sha256=tools["chialisp-oxide-driver"]["sha256"], toolchain=toolchain(),
                current_authoring=lambda: c.private_snapshot(package), optimize=mode == "on",
                rue_revision="245c107a8e795229ea08097142f005a4c0d91075", cargo_options=c.options(package))
            c.strict_abi(loaded.program, name)
            c.require(len(loaded.manifest["entries"]) == 4 and (checked is None or checked.manifest_path == loaded.manifest_path), "complete stable bundle")
            checked = loaded
        destination = output / "bundles" / mode
        file_paths = c.source_files(checked.manifest_path.parent.parent, checked.manifest_path.parent.name, suffixes=None,
                                   limits=c.WalkLimits(depth=3, sources=128, directories=8, entries=256))
        total = 0
        for source in file_paths:
            source = checked.manifest_path.parent.parent / source
            payload = c.read(source, min(16 * 1024 * 1024, 64 * 1024 * 1024 - total))
            total += len(payload)
            path = destination / source.relative_to(checked.manifest_path.parent)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(payload)
        entries = {row["entry"].split("::")[-2]: row for row in checked.manifest["entries"]}
        recompiled = []
        for spec in c.witnesses():
            name, entry = spec["name"], entries[spec["name"]]
            artifacts = json.loads(execute(f"recompile-{mode}-{name}",
                [target / "debug/chialisp-oxide-verifier", destination / entry["ir"], "--artifacts"]), object_pairs_hook=c.cargo_authoring.unique_object)
            selected = next(row for row in artifacts if row["optimize"] is (mode == "on"))
            c.require(all(selected[key] == entry["artifact"][key] for key in ("clvm_hex", "tree_hash", "bytes")), "Cargo/IR artifact replay mismatch")
            recompiled.append(dict(name=name, artifacts=artifacts))
            inputs = output / "inputs" / f"{mode}-{name}.json"
            inputs.parent.mkdir(exist_ok=True)
            c.write(inputs, [spec["solution"]])
            execution = json.loads(execute(f"vm-{mode}-{name}", [target / "debug/chialisp-oxide", "run-batch",
                destination / entry["clvm"], "--solutions", inputs, "--max-cost", "10000000"]))
            c.require(type(execution) is list and len(execution) == 1 and c.check_execution(execution[0]) == spec["expected"], "independent source output witness")
            runs.append(dict(mode=mode, name=name, solution=spec["solution"], module_tree_hash=entry["artifact"]["tree_hash"], execution=execution[0]))
            ledger_runs.append(dict(mode=mode, name=name, module_hex=entry["artifact"]["clvm_hex"]))
        bundles[mode] = dict(manifest=f"bundles/{mode}/manifest.json", manifest_sha256=c.sha(destination / "manifest.json"), recompiled=recompiled)
    request = output / "ledger-request.json"
    c.write(request, dict(protocol="chialisp-oxide-precommit-source-controls-ledger/v1", runs=ledger_runs))
    ledger_bytes = execute("sdk-ledger", [target / "debug/examples/precommit_controls", request])
    (output / "ledger.json").write_bytes(ledger_bytes)
    context = dict(source_root=str(c.ROOT), package_root=str(package), compiler=compiler, toolchain=toolchain(),
                   tools=tools, ledger_runner=ledger_runner, source_window=window)
    files = c.source_files(output.parent, output.name, suffixes=None,
                           limits=c.WalkLimits(depth=5, directories=16, entries=1024, sources=512))
    report = dict(schema_version=1, protocol=c.PROTOCOL, status="actual_source_controls_captured", scope=c.SCOPE, context=context,
        native=dict(test_groups=1, observations=c.native_rows(c.read(output / "native.log").decode()), log="native.log", log_sha256=c.sha(output / "native.log")),
        bundles=bundles, runs=runs, ledger=json.loads(ledger_bytes, object_pairs_hook=c.cargo_authoring.unique_object), commands=commands,
        files=[dict(path=Path(path).relative_to(output.name).as_posix(), sha256=c.sha(output.parent / path)) for path in files])
    unchanged()
    c.write(output / "report.json", report)
    c.validate(report, output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    phases = parser.add_subparsers(dest="phase", required=True)
    p = phases.add_parser("prepare")
    p.add_argument("--package", type=Path, required=True)
    p.add_argument("--receipt", type=Path, required=True)
    n = phases.add_parser("native")
    n.add_argument("--package", type=Path, required=True)
    n.add_argument("--prepared", type=Path, required=True)
    n.add_argument("--target", type=Path, required=True)
    n.add_argument("--output", type=Path, required=True)
    s = phases.add_parser("capture")
    for name in ("package", "native-receipt", "last-host-window", "target", "output"):
        s.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    values = [value.resolve() for key, value in vars(args).items() if key != "phase"]
    try:
        {"prepare": prepare, "native": native, "capture": capture}[args.phase](*values)
    except BaseException as error:
        if hasattr(args, "output") and args.output.exists():
            c.write(args.output / "incomplete-failure.json", dict(status="incomplete_no_success_credit", error=str(error)))
        raise
    print(json.dumps(dict(status=args.phase + "_passed", scope=c.SCOPE)))


if __name__ == "__main__":
    main()
