#!/bin/bash
# Capture the complete current-corpus evidence after the LAST host build.
#
# Run this after every source change that is final (Rust, scripts, puzzles).
# It performs, in the only order that keeps tool identities consistent:
#   1. format check and an initial workspace host build (bins + examples);
#   2. the full corpus validation twice, requiring byte-identical reports;
#   3. fresh Cargo bundles in both optimizer modes;
#   4. the pay-to-singleton policy-control sidecar for this source window;
#   5. the generic corpus-append capture (--record) and its pin;
#   6. the delegated-conditions source-control packet (prepare, native,
#      LAST-host quiet receipt, capture, install);
#   7. the separate boomerang source-control packet, with native policy checks,
#      its own LAST-host quiet receipt, capture, independent check and install;
#   8. the separate precommit source-control packet, with native policy checks,
#      its own LAST-host quiet receipt, capture, independent check and install;
#   9. derived reports: error study, IR/tree/byte proofs, acyclic-scalar and
#      Lean records (the Lean binary must be the pinned 4.35.0 release);
#  10. a host rebuild, fresh bundles and the fresh-bundle gates, because the
#      derived-report producers relink the driver (CI runs them in this order).
# The script stops at the first failure. It never pushes, never edits sources
# beyond the recorded profiles, and establishes no execution authentication.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
WORK=${CAPTURE_WORK:-"$ROOT/target/capture-current-corpus"}
LEAN=${LEAN:-lean}
DRIVER=target/debug/chialisp-oxide-driver
if [[ -e "$WORK" || -L "$WORK" ]]; then
  echo "Capture path already exists: $WORK. Choose a new CAPTURE_WORK; prior runs are preserved." >&2
  exit 1
fi
mkdir -p "$WORK"
step() { echo; echo "##### $*"; }

step "format check and initial host build"
cargo fmt --all --check
cargo build --workspace --bins --examples --locked

step "full corpus validation, twice"
target/debug/chialisp-oxide validate --output corpus/validation.json
target/debug/chialisp-oxide validate --output "$WORK/validation-repeat.json"
cmp corpus/validation.json "$WORK/validation-repeat.json"
python3 scripts/counter_protocol.py --report corpus/validation.json --self-test > "$WORK/gate-counter-protocol.log"
python3 scripts/check-port-evidence.py --report corpus/validation.json --slug counter-increment --slug counter-reset --self-test --replay > "$WORK/gate-counter-ports.log"
python3 -B -m unittest discover -s scripts -p test_ledger_components.py > "$WORK/gate-ledger-components.log" 2>&1

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
step "LAST host build after native policy checks"
cargo build --workspace --bins --examples --locked
python3 scripts/record-controls-quiet-window.py --kind p2dc --package "$WORK/p2dc-package" --native-receipt "$WORK/p2dc-native/native-receipt.json" --output "$WORK/p2dc-last-host.json"
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
import json, os, shutil, time
from pathlib import Path
source, target = Path(os.environ["WORK"]) / "p2dc-capture", Path("corpus/p2dc-source-controls")
report = json.loads((source / "report.json").read_bytes())
# Managed Cargo caches can disappear shortly after their unmount/removal.
# Bound the retry; preserve the captured packet if the old directory stays busy.
for attempt in range(5):
    shutil.rmtree(target, ignore_errors=True)
    if not target.exists():
        break
    time.sleep(3)
if target.exists():
    raise SystemExit(f"{target} still contains files after removal (ignored build output left inside the packet?); remove it and rerun the install step")
target.mkdir()
shutil.copy2(source / "report.json", target / "report.json")
for row in report["files"]:
    destination = target / row["path"]; destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source / row["path"], destination)
PYTHON
python3 scripts/check-p2dc-source-controls.py corpus/p2dc-source-controls/report.json --self-test > "$WORK/gate-p2dc-installed.log"

step "boomerang source-control packet"
python3 scripts/run-boomerang-source-controls.py prepare --package "$WORK/boomerang-package" --receipt "$WORK/boomerang-prepare.json"
python3 scripts/run-boomerang-source-controls.py native --package "$WORK/boomerang-package" --prepared "$WORK/boomerang-prepare.json" --target "$WORK/boomerang-native-target" --output "$WORK/boomerang-native"
step "LAST host build after boomerang native checks"
cargo build --workspace --bins --examples --locked
python3 scripts/record-controls-quiet-window.py --kind boomerang --package "$WORK/boomerang-package" --native-receipt "$WORK/boomerang-native/native-receipt.json" --output "$WORK/boomerang-last-host.json"
python3 scripts/run-boomerang-source-controls.py capture --package "$WORK/boomerang-package" --native-receipt "$WORK/boomerang-native/native-receipt.json" --last-host-window "$WORK/boomerang-last-host.json" --target "$ROOT/target" --output "$WORK/boomerang-capture"
python3 scripts/check-boomerang.py --source-controls "$WORK/boomerang-capture/report.json" --self-test --replay > "$WORK/gate-boomerang-capture.log"
# Apply the same ignored-build-output cleanup as the delegated packet above.
# A successful capture must survive separately even if old mounted output is busy.
for attempt in 1 2 3 4 5; do
  mount | awk -v prefix="$ROOT/corpus/boomerang-source-controls/" 'index($3, prefix) == 1 { print $3 }' | while read -r point; do
    diskutil unmount "$point" >/dev/null 2>&1 || umount "$point" >/dev/null 2>&1 || true
  done
  rm -rf corpus/boomerang-source-controls 2>/dev/null && break
  sleep 3
done
WORK="$WORK" python3 - <<'PYTHON'
import json, os, shutil, time
from pathlib import Path
source, target = Path(os.environ["WORK"]) / "boomerang-capture", Path("corpus/boomerang-source-controls")
report = json.loads((source / "report.json").read_bytes())
# Managed Cargo caches can disappear shortly after their unmount/removal.
# Bound the retry; preserve the captured packet if the old directory stays busy.
for attempt in range(5):
    shutil.rmtree(target, ignore_errors=True)
    if not target.exists():
        break
    time.sleep(3)
if target.exists():
    raise SystemExit("Retained boomerang packet removal failed; preserve this capture and remove only its ignored build output before retrying install")
target.mkdir()
shutil.copy2(source / "report.json", target / "report.json")
for row in report["files"]:
    destination = target / row["path"]; destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source / row["path"], destination)
PYTHON
python3 scripts/check-boomerang.py --source-controls corpus/boomerang-source-controls/report.json --self-test > "$WORK/gate-boomerang-installed.log"

step "precommit source-control packet"
python3 scripts/run-precommit-source-controls.py prepare --package "$WORK/precommit-package" --receipt "$WORK/precommit-prepare.json"
python3 scripts/run-precommit-source-controls.py native --package "$WORK/precommit-package" --prepared "$WORK/precommit-prepare.json" --target "$WORK/precommit-native-target" --output "$WORK/precommit-native"
cargo build --workspace --bins --examples --locked
python3 scripts/record-controls-quiet-window.py --kind precommit --package "$WORK/precommit-package" --native-receipt "$WORK/precommit-native/native-receipt.json" --output "$WORK/precommit-last-host.json"
python3 scripts/run-precommit-source-controls.py capture --package "$WORK/precommit-package" --native-receipt "$WORK/precommit-native/native-receipt.json" --last-host-window "$WORK/precommit-last-host.json" --target "$ROOT/target" --output "$WORK/precommit-capture"
WORK="$WORK" python3 - <<'PYTHON'
import os, shutil
from pathlib import Path
source = Path(os.environ["WORK"]) / "precommit-capture"
target = Path("corpus/precommit-source-controls")
shutil.rmtree(target, ignore_errors=True)
if target.exists():
    raise SystemExit("Old precommit packet is busy; preserve the new capture and inspect ignored build output before installing")
shutil.copytree(source, target)
PYTHON
python3 scripts/check-precommit.py --source-controls-only --self-test --replay > "$WORK/gate-precommit.log"

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
step "complete generator costs (isolated Cargo package; no host-driver rebuild)"
cargo build --manifest-path experiments/spend-costs/Cargo.toml --locked --target-dir "$WORK/spend-costs-target"
python3 experiments/spend-costs/check.py --self-test --record --execute "$WORK/spend-costs-target/debug/chialisp-oxide-spend-costs" --output "$WORK/generator-cost-execution" > "$WORK/gate-generator-costs.log"
echo
echo "Capture complete. Review the diff, then run the remaining CI gates and the site pipeline (pnpm --dir www check && build && test:site) before committing."
