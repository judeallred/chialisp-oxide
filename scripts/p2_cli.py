"""Explicit fresh22 bundle checks for offline historical comparison CLIs.

No extraction or VM runs here. Only requested fresh bundle checks query the
installed pinned rustc identity and hash the caller's driver executable.
"""
from pathlib import Path
import subprocess
import tomllib

import p2_append_migration as append
import runtime_cost_migration as cost


def arguments(parser):
    parser.add_argument("--fresh-on", type=Path, help="explicit current22 optimized Cargo output directory")
    parser.add_argument("--fresh-off", type=Path, help="explicit current22 unoptimized Cargo output directory")
    parser.add_argument("--driver", type=Path, default=cost.ROOT / "target/debug/chialisp-oxide-driver",
                        help="actual driver to hash for explicit fresh22 bundle checks")


def check(report, args):
    """Run current validation and paired fresh checks before any projection."""
    requested = args.fresh_on is not None or args.fresh_off is not None
    cost.require(not append.is_append(report) or not args.ir_bundle,
                 "Current22 requires paired --fresh-on/--fresh-off; --ir-bundle is historical21 only")
    if not requested:
        return None
    cost.require(args.fresh_on is not None and args.fresh_off is not None,
                 "Fresh22 checks require both --fresh-on and --fresh-off")
    cost.require(append.is_append(report), "Fresh22 options require the reviewed current22 append report")
    cost.validate_current(report)
    toolchain = tomllib.loads((cost.ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    compiler = subprocess.run(["rustup", "run", toolchain, "rustc", "-vV"], cwd=cost.ROOT,
                              text=True, capture_output=True, timeout=30)
    cost.require(compiler.returncode == 0 and not compiler.stderr, "Cannot independently query pinned compiler")
    driver = args.driver.resolve(strict=True)
    return append.verify_fresh_bundles(report, args.fresh_on.resolve(), args.fresh_off.resolve(),
        compiler=compiler.stdout, driver_sha256=cost.sha(driver.read_bytes()),
        toolchain=toolchain, cargo_options=["--manifest-path", "puzzles/Cargo.toml",
            "--package", "chialisp-oxide-puzzles", "--locked"])
