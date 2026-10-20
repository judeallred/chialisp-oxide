#!/bin/bash
# Capture the complete current-corpus evidence after the LAST host build.
#
# Run this after every source change that is final (Rust, scripts, puzzles).
# It performs, in the only order that keeps tool identities consistent:
#   1. format check and the LAST workspace host build (bins + examples);
#   2. the full corpus validation twice, requiring byte-identical reports;
#   3. fresh Cargo bundles in both optimizer modes;
#   4. the pay-to-singleton policy-control sidecar for this source window;
#   5. the generic corpus-append capture (--record) and its pin;
#   6. the delegated-conditions source-control packet (prepare, native,
#      LAST-host quiet receipt, capture, install);
#   7. derived reports: error study, IR/tree/byte proofs, acyclic-scalar and
#      Lean records (the Lean binary must be the pinned 4.35.0 release);
#   8. a host rebuild, fresh bundles and the fresh-bundle gates, because the
#      derived-report producers relink the driver (CI runs them in this order).
# The script stops at the first failure. It never pushes, never edits sources
# beyond the recorded profiles, and establishes no execution authentication.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
WORK=${CAPTURE_WORK:-"$ROOT/target/capture-current-corpus"}
LEAN=${LEAN:-lean}
DRIVER=target/debug/chialisp-oxide-driver
rm -rf "$WORK" && mkdir -p "$WORK"
step() { echo; echo "##### $*"; }

step "format check and LAST host build"
cargo fmt --all --check
cargo build --workspace --bins --examples --locked

step "full corpus validation, twice"
target/debug/chialisp-oxide validate --output corpus/validation.json
target/debug/chialisp-oxide validate --output "$WORK/validation-repeat.json"
cmp corpus/validation.json "$WORK/validation-repeat.json"

step "fresh bundles"
target/debug/cargo-chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --output-dir "$WORK/puzzles-on"
target/debug/cargo-chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --no-optimize --output-dir "$WORK/puzzles-off"

step "pay-to-singleton control sidecar"
python3 scripts/capture-p2-controls.py --bundle "$WORK/p2-controls-bundle" --replay "$WORK/p2-controls-replay.json" --driver "$DRIVER"

step "generic corpus append capture"
python3 scripts/check-corpus-append.py --record --fresh-on "$WORK/puzzles-on" --fresh-off "$WORK/puzzles-off" --driver "$DRIVER" | tee "$WORK/record.json"
PIN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["new_pin"])' "$WORK/record.json")
sed -i.bak "s/^PIN = '[0-9a-f]*'/PIN = '$PIN'/" scripts/corpus_append.py && rm scripts/corpus_append.py.bak
python3 scripts/check-corpus-append.py --self-test --fresh-on "$WORK/puzzles-on" --fresh-off "$WORK/puzzles-off" --driver "$DRIVER" > "$WORK/gate-corpus-append.log"

step "delegated source-control packet"
python3 scripts/run-p2dc-source-controls.py prepare --package "$WORK/p2dc-package" --receipt "$WORK/p2dc-prepare.json"
python3 scripts/run-p2dc-source-controls.py native --package "$WORK/p2dc-package" --prepared "$WORK/p2dc-prepare.json" --target "$WORK/p2dc-native-target" --output "$WORK/p2dc-native"
WORK="$WORK" python3 - <<'PYTHON'
import json, os, subprocess, sys, tomllib
from pathlib import Path
sys.path.insert(0, "scripts")
import p2dc_source_controls as controls
root = Path.cwd().resolve()
work = Path(os.environ["WORK"]).resolve()
package, native_path, destination = work / "p2dc-package", work / "p2dc-native/native-receipt.json", work / "p2dc-last-host.json"
controls.require(controls.ROOT == root and not destination.exists(), "NEW quiet receipt required")
toolchain = tomllib.loads(controls.read(root / "rust-toolchain.toml").decode())["toolchain"]["channel"]
window = controls.source_window()
native = controls.load(native_path)
controls.require(native["status"] == "native_phase_passed_before_LAST_host_build" and native["source_root"] == str(root)
    and native["package_root"] == str(package) and native["toolchain"] == toolchain and native["source_window"] == window,
    "Native phase and terminal host build must share exact source inputs")
compiler = subprocess.run(["rustup", "run", toolchain, "rustc", "-vV"], cwd=root, capture_output=True, text=True, timeout=30)
controls.require(compiler.returncode == 0 and not compiler.stderr, "Cannot independently query pinned compiler")
controls.compiler_host(compiler.stdout)
names = ("cargo-chialisp-oxide", "chialisp-oxide-driver", "chialisp-oxide", "chialisp-oxide-verifier")
controls.require(controls.TOOLS == names, "Closed four-tool capture required")
tools = {name: {"path": str(root / "target/debug" / name), "sha256": controls.sha(root / "target/debug" / name, 2 * 1024 * 1024 * 1024)} for name in names}
controls.require(controls.source_window() == window and all(controls.sha(Path(row["path"]), 2 * 1024 * 1024 * 1024) == row["sha256"] for row in tools.values()),
    "Source/tool inputs changed after terminal host build")
receipt = {"schema_version": 1, "protocol": "chialisp-oxide-p2dc-controls-quiet-window/v1",
    "status": "LAST_host_build_terminal_tools_quiet", "source_root": str(root), "package_root": str(package),
    "compiler": compiler.stdout, "toolchain": toolchain, "source_window": window, "tools": tools,
    "native_receipt_sha256": controls.sha(native_path),
    "scope": "Sequential local coordination after the successful workspace --bins --examples host build and before fresh capture. No concurrent builds are started in this interval. This receipt does not authenticate execution or prove compiler preservation."}
destination.write_text(json.dumps(receipt, indent=2) + "\n")
PYTHON
python3 scripts/run-p2dc-source-controls.py capture --package "$WORK/p2dc-package" --native-receipt "$WORK/p2dc-native/native-receipt.json" --last-host-window "$WORK/p2dc-last-host.json" --target "$ROOT/target" --output "$WORK/p2dc-capture"
python3 scripts/check-p2dc-source-controls.py "$WORK/p2dc-capture/report.json" --self-test > "$WORK/gate-p2dc-capture.log"
# Ignored build output can appear inside the retained packet (its package is a
# Cargo project), and some hosts mount dedicated volumes onto Cargo target
# directories. Unmount anything below the packet, then remove it with retries.
for attempt in 1 2 3 4 5; do
  mount | awk -v prefix="$ROOT/corpus/p2dc-source-controls/" 'index($3, prefix) == 1 { print $3 }' | while read -r point; do
    diskutil unmount "$point" >/dev/null 2>&1 || umount "$point" >/dev/null 2>&1 || true
  done
  rm -rf corpus/p2dc-source-controls 2>/dev/null && break
  sleep 3
done
WORK="$WORK" python3 - <<'PYTHON'
import json, os, shutil
from pathlib import Path
source, target = Path(os.environ["WORK"]) / "p2dc-capture", Path("corpus/p2dc-source-controls")
report = json.loads((source / "report.json").read_bytes())
shutil.rmtree(target, ignore_errors=True)
if target.exists():
    raise SystemExit(f"{target} still contains files after removal (ignored build output left inside the packet?); remove it and rerun the install step")
target.mkdir()
shutil.copy2(source / "report.json", target / "report.json")
for row in report["files"]:
    destination = target / row["path"]; destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source / row["path"], destination)
PYTHON
python3 scripts/check-p2dc-source-controls.py corpus/p2dc-source-controls/report.json --self-test > "$WORK/gate-p2dc-installed.log"

step "derived reports"
python3 scripts/check-error-study.py --write
python3 scripts/check-error-study.py
python3 scripts/check-ir-proofs.py --output corpus/proofs.json
python3 scripts/check-tree-proofs.py --output corpus/tree-proofs.json
python3 scripts/check-byte-proofs.py --output corpus/byte-proofs.json
python3 scripts/check-integer-canonicality.py
rm -rf proofs/acyclic-scalar-source
python3 scripts/check-acyclic-source.py --acyclic-scalar --execute --self-test --tools target/debug --target-dir "$WORK/acyclic-scalar-target" --output proofs/acyclic-scalar-source
python3 experiments/tree-theorems/check-model.py --lean "$LEAN" --cli target/debug/chialisp-oxide --self-test --replay-vm --output "$WORK/tree-theorems.json"
cp "$WORK/tree-theorems.json" experiments/tree-theorems/results.json
python3 scripts/restamp-tutorial-rows.py "$ROOT"
python3 corpus/validate_tutorial_examples.py

step "host rebuild, fresh bundles, fresh gates (CI order)"
cargo build --workspace --bins --examples --locked
rm -rf "$WORK/puzzles-on" "$WORK/puzzles-off"
target/debug/cargo-chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --output-dir "$WORK/puzzles-on"
target/debug/cargo-chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --no-optimize --output-dir "$WORK/puzzles-off"
FRESH="--fresh-on $WORK/puzzles-on --fresh-off $WORK/puzzles-off --driver $DRIVER"
python3 scripts/check-corpus-append.py --self-test $FRESH > "$WORK/gate-corpus-append-final.log"
python3 scripts/check-runtime-composition.py --self-test $FRESH > "$WORK/gate-composition.log"
python3 scripts/check-p2-source-cost.py $FRESH --execute-costs --cost-runner target/debug/examples/p2_singleton_cost --execution-output "$WORK/current-p2-cost-execution" > "$WORK/gate-source-cost.log"
python3 scripts/check-cost-optimization.py --self-test --replay $FRESH > "$WORK/gate-cost.log"
python3 scripts/check-helper-inlining.py --self-test --replay $FRESH > "$WORK/gate-inline.log"
python3 scripts/check-runtime-refinement.py --self-test --replay $FRESH > "$WORK/gate-refinement.log"
echo
echo "Capture complete. Review the diff, then run the remaining CI gates and the site pipeline (pnpm --dir www check && build && test:site) before committing."
