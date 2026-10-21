#!/usr/bin/env python3
"""Record a coordinator's quiet window after its successful LAST host build.

This checks source/tool/native-receipt consistency. The caller is responsible
for serializing builds and extraction; the receipt does not authenticate runs.
"""
import argparse
import importlib
import json
from pathlib import Path
import subprocess
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kind", choices=("p2dc", "boomerang"), required=True)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--native-receipt", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    controls = importlib.import_module(args.kind + "_source_controls")
    root = Path.cwd().resolve()
    package = args.package.resolve(strict=True)
    native_path = args.native_receipt.resolve(strict=True)
    destination = args.output.resolve()
    controls.require(controls.ROOT == root and not destination.exists(), "NEW coordinator quiet receipt required")
    toolchain = tomllib.loads(controls.read(root / "rust-toolchain.toml").decode())["toolchain"]["channel"]
    window = controls.source_window()
    native = controls.load(native_path)
    controls.require(type(native["schema_version"]) is int and native["schema_version"] == 1
        and native["protocol"] == f"chialisp-oxide-{args.kind}-controls-native/v1"
        and native["status"] == "native_phase_passed_before_LAST_host_build"
        and native["source_root"] == str(root) and native["package_root"] == str(package)
        and native["toolchain"] == toolchain and native["source_window"] == window,
        "Native phase and terminal host build must share exact source inputs")
    compiler = subprocess.run(["rustup", "run", toolchain, "rustc", "-vV"],
        cwd=root, capture_output=True, text=True, timeout=30)
    controls.require(compiler.returncode == 0 and not compiler.stderr, "Cannot independently query pinned compiler")
    driver = subprocess.run([root / "target/debug/chialisp-oxide-driver", "--compiler-version"],
        cwd=root, capture_output=True, text=True, timeout=30)
    controls.require(driver.returncode == 0 and not driver.stderr
        and driver.stdout.strip() == compiler.stdout.splitlines()[0], "Independent driver/compiler versions differ")
    names = ("cargo-chialisp-oxide", "chialisp-oxide-driver", "chialisp-oxide", "chialisp-oxide-verifier")
    controls.require(controls.TOOLS == names, "Closed four-tool capture required")
    tools = {name: {"path": str(root / "target/debug" / name),
        "sha256": controls.sha(root / "target/debug" / name, 2 * 1024 * 1024 * 1024)} for name in names}
    controls.require(controls.source_window() == window
        and all(controls.sha(Path(row["path"]), 2 * 1024 * 1024 * 1024) == row["sha256"] for row in tools.values()),
        "Source/tool inputs changed after terminal host build")
    receipt = {"schema_version": 1, "protocol": f"chialisp-oxide-{args.kind}-controls-quiet-window/v1",
        "status": "LAST_host_build_terminal_tools_quiet", "source_root": str(root), "package_root": str(package),
        "compiler": compiler.stdout, "toolchain": toolchain, "source_window": window, "tools": tools,
        "native_receipt_sha256": controls.sha(native_path),
        "scope": "Sequential coordinator record after its successful workspace --bins --examples host build and before fresh capture. The caller keeps builds serialized. This receipt does not authenticate execution or prove compiler preservation."}
    destination.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
