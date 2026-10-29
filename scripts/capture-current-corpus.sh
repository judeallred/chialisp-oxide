#!/bin/bash
# Capture the complete current-corpus evidence after the LAST host build.
#
# Run this after every source change that is final (Rust, scripts, puzzles).
# It performs, in the only order that keeps tool identities consistent:
#   1. format check and an initial workspace host build (bins + examples);
#   2. fresh counter controls, then corpus validation twice (byte-identical);
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

capture_counter() {
  local suffix="$1"
  local -a counter_args
  step "fresh counter source controls ($suffix)"
  counter_args=(--output "$WORK/counter-source-capture-$suffix" --native-target "${COUNTER_NATIVE_TARGET:-$WORK/counter-native-target}" --compiler-target "$ROOT/target" --compiler-root "$ROOT" --rustc "$(rustup which --toolchain nightly-2026-07-14 rustc)" --driver "$ROOT/$DRIVER" --cargo-cli "$ROOT/target/debug/cargo-chialisp-oxide")
  python3 -B experiments/action-counter/capture.py prepare "${counter_args[@]}"
  python3 -B experiments/action-counter/capture.py native "${counter_args[@]}"
  cargo build --workspace --bins --examples --locked
  python3 -B experiments/action-counter/capture.py extract "${counter_args[@]}"
  python3 -B experiments/action-counter/capture.py replay "${counter_args[@]}"
  python3 -B experiments/action-counter/protocol_capture.py run --protocol-output "$WORK/counter-protocol-capture-$suffix" "${counter_args[@]}"
  python3 -B scripts/check-action-counter.py --install "$WORK/counter-source-packet-$suffix" --raw-capture "$WORK/counter-source-capture-$suffix" --protocol-capture "$WORK/counter-protocol-capture-$suffix" --self-test --replay --cli target/debug/chialisp-oxide > "$WORK/gate-counter-source-capture-$suffix.log"
  WORK="$WORK" COUNTER_CAPTURE_SUFFIX="$suffix" python3 - <<'PYTHON'
import os, shutil
from pathlib import Path
work = Path(os.environ["WORK"])
suffix = os.environ["COUNTER_CAPTURE_SUFFIX"]
destination = Path("corpus/action-counter-source-controls")
if destination.is_symlink():
    raise SystemExit("Counter packet destination must not be a symlink")
if destination.exists():
    destination.rename(work / f"previous-counter-source-packet-{suffix}")
shutil.copytree(work / f"counter-source-packet-{suffix}", destination)
PYTHON
  python3 -B scripts/check-action-counter.py > "$WORK/gate-counter-source-installed-$suffix.log"
}

capture_counter before-validation

step "full corpus validation, twice"
target/debug/chialisp-oxide validate --output corpus/validation.json
target/debug/chialisp-oxide validate --output "$WORK/validation-repeat.json"
cmp corpus/validation.json "$WORK/validation-repeat.json"
python3 scripts/registry_protocol.py --report corpus/validation.json --repeat-report "$WORK/validation-repeat.json" --self-test > "$WORK/gate-registry-protocol.log"
python3 scripts/check-port-evidence.py --report corpus/validation.json --slug registry-initialize --slug registry-set-alpha --slug registry-set-beta --self-test --replay > "$WORK/gate-registry-ports.log"
python3 scripts/counter_protocol.py --report corpus/validation.json --self-test > "$WORK/gate-counter-protocol.log"
python3 scripts/check-port-evidence.py --report corpus/validation.json --slug counter-increment --slug counter-reset --self-test --replay > "$WORK/gate-counter-ports.log"
python3 -B -m unittest discover -s scripts -p test_ledger_components.py > "$WORK/gate-ledger-components.log" 2>&1

step "fresh bundles"
target/debug/cargo-chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --output-dir "$WORK/puzzles-on"
target/debug/cargo-chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --no-optimize --output-dir "$WORK/puzzles-off"

step "pay-to-singleton control sidecar"
python3 scripts/capture-p2-controls.py --bundle "$WORK/p2-controls-bundle" --replay "$WORK/p2-controls-replay.json" --driver "$DRIVER"

step "fresh CAT source controls before current append admission"
python3 -B experiments/cat-genesis-tail/rust_capture.py prepare --output "$WORK/cat-source-capture" --native-target "$ROOT/target"
python3 -B experiments/cat-genesis-tail/rust_capture.py native --output "$WORK/cat-source-capture" --native-target "$ROOT/target"
cargo build --workspace --bins --examples --locked
python3 -B experiments/cat-genesis-tail/rust_capture.py extract --output "$WORK/cat-source-capture" --native-target "$ROOT/target" --compiler-target "$ROOT/target" --compiler-root "$ROOT" --driver "$ROOT/target/debug/chialisp-oxide-driver" --registry-cli "$ROOT/target/debug/cargo-chialisp-oxide" --cli "$ROOT/target/debug/chialisp-oxide"
python3 -B experiments/cat-genesis-tail/rust_capture.py execute --output "$WORK/cat-source-capture" --native-target "$ROOT/target"
python3 scripts/check-cat-source-controls.py --install-capture "$WORK/cat-source-capture" --packet "$WORK/cat-source-packet" --self-test > "$WORK/gate-cat-source-capture.log"
WORK="$WORK" python3 - <<'PYTHON'
import os, shutil
from pathlib import Path
work = Path(os.environ["WORK"])
destination = Path("corpus/cat-genesis-source-controls")
if destination.is_symlink():
    raise SystemExit("CAT packet destination must not be a symlink")
if destination.exists():
    destination.rename(work / "previous-cat-source-packet")
shutil.copytree(work / "cat-source-packet", destination)
PYTHON
python3 scripts/check-cat-source-controls.py > "$WORK/gate-cat-source-installed.log"

step "generic corpus append capture"
cp scripts/corpus_append.py "$WORK/corpus-append-before-record.py"
python3 scripts/check-corpus-append.py --record --fresh-on "$WORK/puzzles-on" --fresh-off "$WORK/puzzles-off" --driver "$DRIVER" | tee "$WORK/record.json"
PIN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["new_pin"])' "$WORK/record.json")
sed -i.bak "s/^PIN = '[0-9a-f]*'/PIN = '$PIN'/" scripts/corpus_append.py && rm scripts/corpus_append.py.bak
# The counter's broad current-source window includes corpus_append.py. A
# changed reviewed pin requires new native/extraction/VM/SDK evidence, retaining
# both attempts and the previously installed packet rather than patching hashes.
if ! cmp -s "$WORK/corpus-append-before-record.py" scripts/corpus_append.py; then
  capture_counter after-pin
fi
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

step "registry Rust source-control native checks and LAST host build"
python3 scripts/run-registry-source-controls.py native --execute --target "$WORK/registry-native-target" --output "$WORK/registry-native"
python3 scripts/run-registry-source-controls.py last --execute --target "$ROOT/target" --native-dir "$WORK/registry-native" --output "$WORK/registry-last"
step "actual registry triplet base and own-root source controls"
target/debug/chialisp-oxide validate --only registry-initialize,registry-set-alpha,registry-set-beta --output "$WORK/registry-base/report.json"
python3 scripts/run-registry-source-controls.py capture --execute --native-dir "$WORK/registry-native" --last-dir "$WORK/registry-last" --base-report "$WORK/registry-base/report.json" --output "$WORK/registry-capture"
python3 scripts/check-registry-source-controls.py --install-capture "$WORK/registry-capture" --native-dir "$WORK/registry-native" --last-dir "$WORK/registry-last" --base-dir "$WORK/registry-base" --packet "$WORK/registry-packet" --report corpus/validation.json > "$WORK/gate-registry-capture.log"
WORK="$WORK" python3 - <<'PYTHON'
import os, shutil
from pathlib import Path
work = Path(os.environ["WORK"])
destination = Path("corpus/registry-source-controls")
if destination.is_symlink():
    raise SystemExit("Registry packet destination must not be a symlink")
if destination.exists():
    destination.rename(work / "previous-registry-packet")
shutil.copytree(work / "registry-packet", destination)
PYTHON
python3 scripts/check-registry-source-controls.py --packet corpus/registry-source-controls --report corpus/validation.json --self-test --replay --cli target/debug/chialisp-oxide > "$WORK/gate-registry-installed.log"

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
