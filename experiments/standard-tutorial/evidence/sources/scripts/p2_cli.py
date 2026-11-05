"""Explicit current22/current23 bundle checks for offline historical comparison CLIs.

No extraction or VM runs here. Only requested fresh bundle checks query the
installed pinned rustc identity and hash the caller's driver executable.
"""
from pathlib import Path
import subprocess
import tomllib

import p2_append_migration as append
import runtime_cost_migration as cost


def arguments(parser):
    parser.add_argument("--fresh-on", type=Path, help="explicit current22/current23 optimized Cargo output directory")
    parser.add_argument("--fresh-off", type=Path, help="explicit current22/current23 unoptimized Cargo output directory")
    parser.add_argument("--fresh-cargo-option", dest="fresh_cargo_options", action="append", help="current23 only: repeat the COMPLETE independently expected Cargo option vector; use = for values beginning with --")
    parser.add_argument("--driver", type=Path, default=cost.ROOT / "target/debug/chialisp-oxide-driver",
                        help="actual driver to hash for explicit fresh22 bundle checks")


def check(report, args):
    """Run current validation and paired fresh checks before any projection."""
    if args.fresh_on is not None or args.fresh_off is not None or getattr(args, 'fresh_cargo_options', None) is not None:
        cost.validate_current(report)
    return _check_after_current_validation(report, args)


def _check_after_current_validation(report, args):
    """Internal: caller has checked the actual current report, never history."""
    requested = args.fresh_on is not None or args.fresh_off is not None or getattr(args, 'fresh_cargo_options', None) is not None
    cost.require(not append.is_append(report) or not args.ir_bundle,
                 "Current append requires paired --fresh-on/--fresh-off; --ir-bundle is historical21 only")
    if not requested:
        return None
    cost.require(args.fresh_on is not None and args.fresh_off is not None,
                 "Fresh append checks require both --fresh-on and --fresh-off")
    cost.require(append.is_append(report), "Fresh options require the reviewed current append report")
    toolchain = tomllib.loads((cost.ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    compiler = subprocess.run(["rustup", "run", toolchain, "rustc", "-vV"], cwd=cost.ROOT,
                              text=True, capture_output=True, timeout=30)
    cost.require(compiler.returncode == 0 and not compiler.stderr, "Cannot independently query pinned compiler")
    driver = args.driver.resolve(strict=True)
    options = ["--manifest-path", "puzzles/Cargo.toml", "--package", "chialisp-oxide-puzzles", "--locked"]
    override = getattr(args, "fresh_cargo_options", None)
    if override is not None:
        import p2dc_append_migration as outer
        cost.require(outer.is_append(report), "Explicit expected Cargo options are current23 only")
        cost.require(type(override) is list and 0 < len(override) <= 64
            and all(type(value) is str and 0 < len(value) <= 4096 for value in override), "Expected Cargo option vector malformed")
        options = list(override)
    return cost.verify_current22_bundles(report, args.fresh_on.resolve(), args.fresh_off.resolve(),
        compiler=compiler.stdout, driver_sha256=cost.sha(driver.read_bytes()),
        toolchain=toolchain, cargo_options=options)
