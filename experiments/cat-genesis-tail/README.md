# Local CAT TAIL rehearsal

WIP prototype; not ready for real funds. The current policy source lives in
`puzzles/cat-genesis-by-coin-id/puzzle.rs`; this ordinary Cargo package imports
that exact file. Its SDK examples call the shared native corpus lifecycle
harness. The paired corpus adds a separate Rue source and five-build main
lifecycle. The older checkpoint and counts below remain historical and do not
receive credit for that promotion.

## Historical canonical genesis-by-coin-ID TAIL preparation

**WIP prototype. Not ready for real funds.** This isolated, unpublished Cargo
library prepares the canonical `genesis_by_coin_id` TAIL as a raw-tree Rust
translation for the later CAT tutorial (#13). It is not registered in the
paired corpus. A separate canonical-only SDK harness now executes issuance,
transfer and a recipient spend in the in-memory simulator.

**Status: native and canonical primary-VM checks executed.** Isolated offline
native tests, formatting and Clippy passed on the pinned nightly with two jobs.
The actual Rust Cargo export produced 14 nil outputs and 23 caught policy
panics over 37 raw environments. A separate test independently compiled the
pinned Chialisp source/include to the published 45 bytes, then executed all 37
vectors in primary `clvmr 0.21.0`: 14 nil outputs, 11 `path_into_atom`, 9 `raise`
and 3 `invalid_op_arg` outcomes. These are two integration-test groups, not
74 complete spends. Separate executed Rust CLVM and SDK lanes are described
below. Corpus integration and live network execution remain pending.

## Exact source and original contract

`upstream/` retains the unchanged module, its complete `cat_truths.clib`
include, published compiled hex and Apache-2.0 license from
`Chia-Network/chia_puzzles` commit
`d122456feeef55708d354a065623037bfa010714`, root Git tree
`89e5f55c5afdde2c436b9a2c08b8e30ecf11eb62`. `provenance.json` pins each
selected upstream path, URL, Git blob identity, SHA-256 and byte count. The
published canonical artifact is 45 bytes with tree hash
`493afb89eed93ab86741b2aa61b8f5de495d33ff9b781dfc8919e602b2afa150`.
Downloading those bytes and checking their hashes is not compilation or
execution authentication. The separate executed reference test supplies the
local compilation observation; neither supplies chain inclusion evidence.

The pre-curry environment is conventionally written:

```text
(GENESIS_ID Truths parent_is_cat lineage_proof delta inner_conditions _)
```

The canonical compiled program uses only three paths:

- Path 47 selects `delta`. Only the nil atom passes; nonempty zero bytes,
  padded numbers and pairs are truthy and raise.
- Path 2 selects `GENESIS_ID`.
- Path 45 selects `first(rest(rest(Truths)))`, the parent field.

The last two atom payloads must match byte for byte. Empty, padded, signed-looking
and arbitrarily wide equal atoms are valid. The policy adds no Bytes32, integer
or exact tuple decoder. A successful result is nil, not a condition list that
independently establishes a complete spend. Truths authenticity, CAT conservation
and allowed issuance depend on the enclosing canonical CAT protocol.

The `parent_is_cat`, lineage, inner-conditions and final arguments are unused by
this specific TAIL. Their values and tails remain opaque. Five argument cells
are enough to reach delta; no extra final arguments or proper tail are required.
Within Truths, only its first two rests and the next first are projected. A
wrapper that validates a complete Truths record or narrows either compared atom
would be an adapter with a different acceptance contract.

## Projection order and failure scopes

`src/lib.rs` uses one whole-environment `&Tree` parameter and ordinary local
projection helpers. It checks delta before attempting the equality. The
published equality expression compares paths `(45, 2)`; CLVM evaluates those
arguments right-to-left. The translation therefore projects genesis first,
then the parent, then checks the resulting atom operands and compares bytes.
It does not check that genesis is an atom before projecting a possibly malformed
parent. No projection of an otherwise unused argument is introduced.

Source `panic!()` preserves rejection but does not supply a typed VM failure
identity. Canonical replay observed `path_into_atom` for missing projections and
`invalid_op_arg` for pair equality operands. The separate actual Rust off/on
replays observed `raise` on all 23 rejected vectors per mode. The lanes compare
acceptance and exact nil output; native panics are not relabeled with canonical
VM codes, and the observed failure-code differences remain explicit. Resource, codec
and infrastructure failures remain inconclusive. The vector file remains an
independent specification; actual typed outcomes are emitted separately as
`CAT_GENESIS_CANONICAL_VM` rows, with `clvmr-0.21.0/v1`, MEMPOOL_MODE and a
1,000,000-cost limit. Every rejected VM case matched its specified program code.

`vectors.py` specifies acceptance over complete raw trees independently of Rust.
Its 37 retained vectors include 0/1/31/32/33/256/257/1024-byte equal atoms,
nil/padded-zero distinctions, malformed environment and Truths paths, pair
operands, improper unused tails and competing failure-order witnesses.
`tests/native.rs` calls the actual Cargo export over these exact bytes; fixture
decoding and its resource bounds sit outside the policy panic adapter. A second
test independently compiles the untouched source with the exact named include,
compares the published bytes and executes those actual compiled bytes on every
vector. Both tests have run; they do not extract or execute a Rust CLVM artifact.

```sh
python3 experiments/cat-genesis-tail/vectors.py
```

That command checks retained provenance, artifact tree-hash arithmetic and
deterministic vector specifications without running a compiler or VM. Use
`--write` only when intentionally regenerating the specification. `mutations.json`
defines unchanged-source, dropped-delta and dropped-equality source controls.
Their actual Rust execution belongs to the separate capture lane below.

## Rust capture protocol

1. Review the raw contract and source recipes. Resolve a pinned isolated Cargo
   lockfile, format Rust, and run ordinary native check/test/Clippy. Capture each
   exact `CAT_GENESIS_NATIVE` observation and the independent source/include
   compilation result. The reference test must reproduce the 45 published bytes.
2. Finish the host build, freeze the complete source/tool inputs, and extract
   `chialisp_oxide_cat_genesis_tail::genesis_by_coin_id` in both Rust modes with
   complete schema-2 source receipts. Keep any generated `Cargo.lock` as part of
   the declared source closure. Do not rebuild a shared driver while executing it.
3. Run every vector through retained canonical bytes and each actual Rust mode.
   Compare acceptance and exact nil output independently; retain typed failure
   codes per implementation and distinguish policy failures from resource limits.
   Recompile the retained IR and replay the retained artifacts separately. No Rue
   source translation exists here, so this is not a five-mode corpus lane.
4. Compile the actual source mutants in isolated Cargo packages. Compare native
   and both-mode VM observations on their distinguishing witnesses. Do not credit
   merely edited JSON or constructed IR as source-mutation execution.

After compilation, each Rust TAIL module has its own module hash. Curry the same
actual genesis coin ID into each selected TAIL before computing its asset ID.
The canonical curried TAIL ID and the Rust curried TAIL IDs must remain distinct;
matching behavior on finite vectors is not canonical CAT asset identity.

## Executed canonical SDK lifecycle, separate from raw TAIL tests

`examples/canonical_lifecycle.rs` independently compiles the pinned original
TAIL and CAT2 sources/includes, checks their published bytes, and uses the
low-level pinned CAT/standard-wallet schemas. `lifecycle-provenance.json` binds
the additional CAT2 source/includes. No high-level SDK driver or project Rust
compiler is involved. The CAT2 module is 1,672 bytes with tree hash
`37bef360ee858133b69d595a906dc45d01af50379dad515eb9518abb7c1d2a7a`.

One canonical lifecycle group funds an Eve from an actual genesis input G,
runs zero-delta TAIL issuance, transfers the issued CAT with its real parent
lineage and one-coin ring, and spends that actual transfer with the recipient's
key and lineage. The selected TAIL is curried with G's exact coin ID before
funding its CAT asset. Each spend records exact coin, reveal, solution, signature,
asset and child identities. Disposable deterministic fixture keys are public.
This is an in-memory SDK run with TESTNET11 consensus constants, not a network
transaction or wallet integration.

Each run contains 23 SDK transaction attempts: 10 accepted, seven exact typed
consensus rejections and six runtime umbrellas. The ten accepted transactions
comprise five ordinary standard-wallet funding transactions and five CAT
transitions; they are not ten CAT spends. The main lifecycle has 15 attempts.
Four source-control groups add eight attempts, including their own funding.
All intended rejected submissions assert full simulator equality in memory.
Missing, wrong-key and wrong-message signatures actually reach consensus and
match `BadAggregateSignature`; ring/imbalance negatives match
`AssertCoinAnnouncementFailed`; the consumed genesis replay matches `DoubleSpend`.

SDK `GeneratorRuntimeError` remains an umbrella, not a typed semantic rejection.
Those six attempts separately require an intended primary-VM program failure.
The 25 component primary-VM spend executions produce 19 accepted condition lists,
five `raise` and one `path_into_atom` outcomes. They use primary `clvmr 0.21.0/v1`,
MEMPOOL_MODE and a 50,000,000-cost bound. The SDK uses `chia-sdk-test 0.36.0`,
consensus `0.36.1` and its distinct `clvmr 0.16.2`; these protocols are explicit
in the retained report. Resource, codec and infrastructure failures cannot satisfy
the intended program rejection.

`lifecycle-controls.json` defines two actual Chialisp source edits, separate from
the independently captured Rust recipes in `mutations.json`. Original and mutant
wrong-genesis witnesses distinguish the parent check. Original and mutant
nonzero-delta witnesses distinguish the nil guard. Every mutant funds its own
changed reveal and asset ID. The delta witness uses CAT input 1,000, CAT output
1,001, extra delta +1 and an actual separate signed one-mojo standard input.
Thus the CAT ring subtotal and XCH conservation hold while the original TAIL
rejects; the dropped-delta TAIL permits the changed supply. This finite control
does not characterize all CAT2 supply policies or multi-input rings.

Reproduce from this sibling with an isolated target and the pinned nightly:

```sh
export CARGO_TARGET_DIR=/absolute/path/to/isolated-cat-target
export CARGO_BUILD_JOBS=2
export CARGO_NET_OFFLINE=true
rustup run nightly-2026-07-14 cargo test --manifest-path experiments/cat-genesis-tail/Cargo.toml --locked
rustup run nightly-2026-07-14 cargo fmt --manifest-path experiments/cat-genesis-tail/Cargo.toml --check
rustup run nightly-2026-07-14 cargo clippy --manifest-path experiments/cat-genesis-tail/Cargo.toml --all-targets --locked -- -D warnings
rustup run nightly-2026-07-14 cargo build --manifest-path experiments/cat-genesis-tail/Cargo.toml --example canonical_lifecycle --locked
"$CARGO_TARGET_DIR/debug/examples/canonical_lifecycle" > lifecycle-report.json 2> lifecycle-trace.log
python3 experiments/cat-genesis-tail/lifecycle.py --report lifecycle-report.json --self-test
```

Freeze source inputs and the built runner around execution, retain failed attempts,
and independently rerun into new paths requiring byte-identical report bytes.
The first local report and repeat logs are retained outside source under
`target/cat-genesis-native-preflight/`; these machine-local logs are not committed
corpus evidence. Repeating the run adds reproducibility evidence, not new
scenario coverage.

The bounded `lifecycle.py` reader checks pinned source and artifact identities,
exact curry framing, named phases, coin/asset/lineage/ring/condition identities,
signature witness consistency, typed outcomes and distinct counts. Its 22
constructed corruption controls include replacing the previous-owner signature
with an empty signature. Reading a report executes no compiler, VM or SDK. A
recorded full-state-equality marker is a consistency claim; it does not serialize
simulator state, authenticate execution or independently prove that assertion.
Source/artifact consistency likewise does not prove compiler preservation.

Remaining work includes reviewed complete corpus capture, broader ring scenarios
and the later Sage/live Testnet11 workflow with authorized external funding and
signer. The separate finite Rust TAIL complete-spend lane is described below.
No Sage interaction, live funding, BLS theorem, whole CAT tutorial completion or
production readiness is claimed here.

## Executed Rust TAIL capture workflow

`rust_capture.py` prepares a separate ordinary Cargo library with three exports:
byte-identical original source and the two exact Rust recipes in `mutations.json`.
The generated native test calls those actual exports: all 37 original vectors
and one independent distinguishing witness per mutant, giving 39 observations
in one executed test group. The script parses the actual test count and exact
observation rows; successful Cargo exit with zero tests cannot satisfy the gate.

Run `prepare`, then `native`, then the exclusively coordinated `extract` phase
with `--output`, `--native-target`, and explicit `--compiler-target`,
`--compiler-root`, `--driver`, `--registry-cli` and `--cli` arguments. Extraction
never rebuilds these supplied tools. It requests complete Cargo source receipts
in both optimization modes, admits every export, independently inspects compiled
module identities and freezes both the sibling dependencies and the actual
supplied compiler repository sources. Ordinary native fmt/test/Clippy and the
last SDK host build remain separate from the project's Rust subset check.

`execute` runs the already-built `rust_lifecycle` SDK harness twice into new
files. It requires byte-identical reports, unchanged source/tool/runner inputs
and the entire retained bundle inventory. `rust_lifecycle.py` checks the recorded
report separately, with corruption controls and no execution credit. The Rust
lane supplies explicit actual artifact identities to the same exact canonical
CAT2 phase checker; it cannot substitute a canonical asset ID for a Rust reveal.

The executed finite scope is 37 canonical raw VM runs, 74 original Rust raw VM
runs across off/on, four mutant raw witnesses, two Rust lifecycle groups and
eight source-control groups. The latter compose 46 SDK submissions and 50
component VM spend executions, distinct from raw TAIL vectors and native tests.
There are 20 accepted transactions (ten standard funding and ten CAT transitions),
14 exact typed consensus rejections and 12 separately checked runtime umbrellas.
Two independent complete reports were byte-identical. The native lane executed
39 observations in one test group. Formatting and Clippy passed. Every original
and mutant funds its own committed reveal and curried asset; off/on identities
are recorded independently and are not assumed to differ from each other.
Canonical projection/type failures and actual Rust panic codes stay separate.
No live Testnet11 wallet, Sage, corpus integration or compiler-preservation claim
follows from this local workflow.

The Rust component profile explicitly allows 250,000,000 primary-VM cost; raw
TAIL vectors keep 1,000,000 and the canonical lifecycle keeps 50,000,000. The
pinned SDK `Simulator::new_transaction` actually validates each bundle at
5,500,000,000, half the TESTNET11 block constant 11,000,000,000. These distinct
limits are recorded; they are not interchangeable charged transaction costs.
The first Rust SDK attempt was retained as inconclusive when the large off-mode
dropped-delta reveal exceeded the old 50,000,000 primary component limit before
SDK submission. It supplies no complete-report or rejection credit. Compiler
and source-vector limits were unchanged. `compiled_codec.py` uses an explicit
compiled transport profile (1 MiB, 65,536 nodes, depth 512, 16 MiB modeled
work), with preflight before decoded-tree allocation and pure corruption checks.

Measured original Rust modules are 2,157 bytes off and 735 bytes on; both have
module hashes and curried asset IDs distinct from canonical Chialisp. The
dropped-delta module is larger: 47,727 bytes off and 13,863 bytes on. Its actual
canonical CAT2 component VM costs were 147,723,611 off and 43,311,309 on, and both
complete SDK mutant submissions were accepted. This is a finite source-control
witness, not an optimization or whole-spend cost improvement claim. The bounded
reader has 13 report corruption controls and six compiled-codec controls, separate
from actual execution. Retained per-attempt source, runner, bundle and report
receipts live under `target/cat-genesis-rust-attempt*/`; failed attempts and the
earlier canonical captures remain unchanged.


## Capture resource and retained-native reader follow-up

The current Python capture runner caps stdout and stderr separately at 16 MiB,
charging each chunk before writing its retained prefix. Commands have a
600-second deadline and five-second cleanup allowance. POSIX process-group
cleanup kills inherited descendants on timeout, excess output, infrastructure
failure and successful parent exit; the direct child is reaped. Descendant
reaping belongs to the OS. This supervises the trusted commands, not programs
that deliberately escape their process group or a hostile filesystem/kernel.
Each started command retains capped logs and a receipt with `passed`/`failed`,
exit status and a distinct `timeout`, `output_limit`, `nonzero_exit`,
`interrupted` or `infrastructure` failure kind. Truncated logs cannot satisfy a
successful native/SDK gate. Filesystem failure can itself prevent receipt writes.

The complete retained-bundle inventory is limited to 4,096 files, 16 MiB per
file and 64 MiB aggregate. The count and every declared stat length are reserved
before any content read or hash; each subsequent read uses only that file's
reservation. Growth or shrinkage fails before hashing. The reviewed reader
source closure now includes `scripts/raw_tree.py`, used by vector provenance.

The native reader independently regenerates the 37 original expected vectors
and two mutant outputs. It requires exact ordered equality with `native.json`,
the retained native stdout rows and its one-test success summary. The native
receipt binds both that stdout and the bounded command receipt; the reader
checks the exact Cargo argv, target, success status, bounds and both log
identities. Coherent forged logs still are not authenticated execution.
Six new native-record corruption controls supplement the existing 13 report
controls. Previously captured native records lack these new bindings and remain
historical; the strengthened reader needs a fresh coordinated capture. No old
capture or failed attempt is rewritten or upgraded by these source changes.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 experiments/cat-genesis-tail/test_rust_capture.py
```

Fifteen lightweight Python controls passed: capped stream and process-group
supervision (including actual Python grandchildren), pre-read inventory budgets,
growth/shrinkage, native witness/stdout/command corruption and read-only reader
operation. Their records are constructed host fixtures and supply no Cargo,
Rust native-policy, driver, VM, SDK or external-network execution credit. The
new 19-control full retained-reader self-test awaits the next CAT capture.
