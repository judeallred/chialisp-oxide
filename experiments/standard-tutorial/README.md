# Standard delegated/hidden tutorial: wallet-free SDK lane

**WIP prototype. Not ready for real funds or production.** This is a dedicated
local simulator exercise around the unchanged pinned tutorial reference and its
actual Cargo Rust and independently authored Rue translations. It is separate
from generic template evidence, the canonical Chia standard module, a complete
wallet implementation and live-network testing.

The lesson is in [standard-tutorial.md](../../docs/standard-tutorial.md). The
original tutorial/code/license/include pins are in the puzzle manifest; no source
wrapper or hidden change to its raw four-field ABI is used. The public seed07
fixture commits a synthetic key before funding each actual module's curried
instance. Its hidden fixed-payment program deliberately adds no signature.

First build and completely admit fresh Cargo bundles in both modes, independently
compile the unchanged Chialisp reference with its declared include mapping, and
compile Rue in both modes. The shared port gate checks source hashes, all named
expected vectors and retained-byte replays. Root's official complete corpus
capture supplies this evidence; the preparer below does not authenticate modules
or substitute for complete bundle admission. A lane label confers no authority.

Given five already admitted exact module hex files:

```sh
python3 experiments/standard-tutorial/prepare_request.py \
  --rust-off RUST_OFF.hex --rust-on RUST_ON.hex --chialisp ORIGINAL.hex \
  --rue-off RUE_OFF.hex --rue-on RUE_ON.hex \
  --output target/standard-tutorial-request.json
cargo run -p chialisp-oxide-corpus --example standard_tutorial_ledger --locked \
  -- target/standard-tutorial-request.json > target/standard-tutorial-report.json
```

The normal Cargo example uses existing exact SDK dependencies and the root
`Cargo.lock`; it needs no new dependency or separate lock. It bounds/canonicalizes
all module bytes, checks expected primary outputs before SDK signer execution,
and bounds expanded outputs before serialization. The report retains each actual
module/reveal, coin identity, solution, signature, complete output and typed primary
observation. Primary VM is clvmr0.21.0; the SDK's VM/consensus version is separately
reported. Resource or infrastructure failures abort inconclusively.

A successful run contains five builds and 80 SDK submissions: 15 accepted,
45 exact typed consensus rejections and 20 runtime umbrellas. Every rejected SDK
submission compares the full in-memory simulator with its pre-submit clone.
Runtime umbrellas add no semantic rejection credit. Five additional contemplated
primary observations demonstrate that signing an unrestricted identity program
can leave the recipient under solution control; these add no SDK submissions.
The three directly minted fixture coins per build are simulator conveniences,
not wallet funding transactions. Repeat the run into a new file and compare exact
bytes; a repeat adds no unique coverage.

Private source-only integration handoffs keep their candidate reports and complete
source/bundle/tool bindings under `target`. An official retained packet, source
freshness/display gates, exact-commit CI/Pages verification and any live experiment
are separate work. No source hash or retained report authenticates execution,
installed binaries, cryptography, nominal Rust types or compiler preservation.

The integrated capture command defaults to a plan and runs no tools. After the
final merged source is frozen and the complete current 48-export corpus and both
complete schema-2 Cargo bundles are freshly captured, run:

```sh
python3 -B experiments/standard-tutorial/capture.py --execute \
  --rust-off FRESH_COMPLETE_OFF_BUNDLE --rust-on FRESH_COMPLETE_ON_BUNDLE \
  --tools CURRENT_TOOL_DIRECTORY --expected-host PINNED_HOST \
  --output target/standard-tutorial-current-capture
python3 -B experiments/standard-tutorial/check.py \
  --packet target/standard-tutorial-current-capture --current --self-test
```

The tool directory must contain the three current compiler CLI/driver executables
and `examples/standard_tutorial_ledger`. Capture builds nothing, runs the SDK
example twice, requires byte equality, and checks the final source/tool windows.
Its new dedicated packet profile is 2,048 files, 16 MiB per file and 32 MiB total,
including the receipt. This does not change the separate registry packet limits.
The packet copies the complete two Cargo bundle inventories, bounded declared
source closure, one-row template projection, request, both reports and logs.
Absolute producer origins remain opaque and are never followed by its reader.

Only after checking the fresh capture should the release owner install its exact
contents at `experiments/standard-tutorial/evidence/`; do not rewrite its producer
origin or relabel the older isolated candidate as current. The default recorded
checker, CI and the site then require this packet; missing current capture is a
pending release gate, not a passing test. Recorded checks execute no SDK, VM,
compiler or BLS verifier and add no transaction attempts. Download routes derive
from the checked closed physical inventory.
