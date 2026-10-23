# Genesis-by-coin-ID CAT TAIL: local Rust lifecycle

**WIP prototype. Not ready for real funds. This is a local experiment using
an in-memory SDK simulator, not a live CAT or Sage integration.** No
wallet, Sage integration or network funding is required for these local checks.

The unpublished ordinary Cargo library
[chialisp-oxide-puzzles](https://github.com/judeallred/chialisp-oxide/tree/main/puzzles)
exports the annotated
[cat-genesis-by-coin-id policy](https://github.com/judeallred/chialisp-oxide/blob/main/puzzles/cat-genesis-by-coin-id/puzzle.rs).
Native execution links that same source. The TAIL is a protocol component:
returning nil alone is not a complete coin spend. A shared host harness composes
each artifact with canonical CAT2, signed standard-wallet funding and real
parent lineage, then spends the resulting CAT child.

The paired manifest declares Rust, unchanged canonical Chialisp and independently
authored Rue source. Each language retains its own module and curried asset
identity. Broader CAT rings, Sage/live Testnet11 workflow and the full acceptance
criteria of issue #13 remain open. Rust using Rue LIR alone would not establish
a Rue source port.

The current paired run checks 37 named raw environments in each of five builds,
for 185 named VM observations. Another 2,059 generated raw environments run in
each build, for 10,295 generated VM observations. The shared port gate regenerates
those inputs and checks them against the independent raw-tree relation.
Its separate CAT2 lifecycle has 75 SDK submissions:
20 accepted (five funding transactions and 15 CAT transitions), 35 exact typed
consensus rejections and 20 runtime umbrellas. The 75 primary component VM
observations are a separate count. These finite local runs require no wallet.

## Exact policy and artifact identities

The unchanged reference module and complete includes are retained from
[Chia-Network/chia_puzzles at d122456feeef55708d354a065623037bfa010714](https://github.com/Chia-Network/chia_puzzles/tree/d122456feeef55708d354a065623037bfa010714)
under Apache-2.0. The selected canonical TAIL is 45 bytes, with module tree hash
`493afb89eed93ab86741b2aa61b8f5de495d33ff9b781dfc8919e602b2afa150`.
The enclosing canonical CAT2 module is 1,672 bytes, with tree hash
`37bef360ee858133b69d595a906dc45d01af50379dad515eb9518abb7c1d2a7a`.
These are distinct dependencies. Neither is an on-chain Rust port of CAT2.

The original contract uses only the nil `delta` and byte equality between the
committed genesis ID and the CAT truth's parent field. Atom payloads can be
empty, padded or wider than 32 bytes. Opaque unused arguments and improper tails
stay opaque. The Rust export therefore uses the whole-environment Tree ABI;
it does not replace this domain with a Bytes32 or integer decoder.

The source checks delta first, then genesis and parent projections. Canonical
Chialisp can report `path_into_atom` or `invalid_op_arg` where native Rust panics
and Rust CLVM raises. Comparison is acceptance plus exact nil output. Typed VM
failures are recorded separately per language and mode; resource, codec and
infrastructure failures supply no rejection credit.

Each Rust mode has its own module identity. Before funding, the selected TAIL
is curried with the actual genesis coin ID and its tree hash becomes that CAT's
asset ID. The Rust and canonical asset IDs remain distinct. Never substitute
the canonical module or asset hash for an incompatible Rust artifact, even when
finite observations agree.

## Build the Cargo export and paired corpus

The annotated export is
`chialisp_oxide_puzzles::cat_genesis_by_coin_id::puzzle`. Its manifest is
[puzzles/cat-genesis-by-coin-id/puzzle.json](https://github.com/judeallred/chialisp-oxide/blob/main/puzzles/cat-genesis-by-coin-id/puzzle.json).
Start with ordinary Cargo, then request the additional supported-subset check:

```sh
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver -p chialisp-oxide-cli
export PATH="$PWD/target/debug:$PATH"
cargo check -p chialisp-oxide-puzzles --locked
cargo chialisp-oxide check --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --output-dir target/cat-corpus-on
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --no-optimize --output-dir target/cat-corpus-off
```

Native Cargo checking verifies Rust; the separate compiler command validates
this project's CLVM subset. The whole corpus capture compiles Chialisp and Rue
independently, executes the declared raw vectors, and submits each build's own
curried TAIL through the shared CAT2 lifecycle. Complete report generation is
coordinated with the other corpus and proof captures; see the
[verification guide](https://judeallred.github.io/chialisp-oxide/verification/method/).

## Run the independent local checks

Keep ordinary native tests separate from the additional CLVM subset check.
Use the pinned dated nightly, explicit rustc and two Cargo jobs:

```sh
export RUSTUP_TOOLCHAIN=nightly-2026-07-14
export RUSTC="$(rustup which --toolchain nightly-2026-07-14 rustc)"
export CARGO_BUILD_JOBS=2
export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR="$PWD/target/cat-genesis-root-native"
cargo fmt --manifest-path experiments/cat-genesis-tail/Cargo.toml --check
cargo test --manifest-path experiments/cat-genesis-tail/Cargo.toml --locked
cargo clippy --manifest-path experiments/cat-genesis-tail/Cargo.toml --all-targets --locked -- -D warnings
python3 -B experiments/cat-genesis-tail/test_rust_capture.py
```

The package's native tests call the original Cargo export and independently
compile the pinned Chialisp TAIL to its exact published bytes. The pure Python
controls test bounded command supervision and retained-reader corruption;
they execute no Rust policy, compiler, VM, SDK or network request.

For a complete Rust capture, run the staged
[rust_capture.py](https://github.com/judeallred/chialisp-oxide/blob/main/experiments/cat-genesis-tail/rust_capture.py)
workflow in a new directory beneath target. `prepare` materializes the exact
original and two source mutants in an ordinary Cargo package. `native` executes
37 original witnesses and one distinguishing witness per mutant, then finishes
the host build. Finish the workspace bins/examples build before `extract`,
reserve the shared driver lane and keep source/tool bytes quiet thereafter.
`extract` requests six complete schema-2 exports: three controls in both modes.
It independently queries compiler/driver identity and checks actual source-read
receipts. `execute` submits the actual already-built SDK harness twice and
requires byte-identical reports plus exact retained source/tool windows.

No phase rebuilds a supplied driver implicitly. Do not run a workspace build
while another process executes that driver. Output, failure logs and host-bound
receipts belong in target; preserve failed attempts and use new capture paths
for retries. The source README documents the explicit tool/path arguments.

The [portable source-control packet](https://judeallred.github.io/chialisp-oxide/reports/cat-source-controls/report.json)
retains complete bundles, exact source recipes and bounded capture receipts.
Its reader checks recorded consistency without running a compiler, VM or SDK:

```sh
python3 scripts/check-cat-source-controls.py --self-test
```

That recorded check adds no execution credit and does not authenticate the
retained tool declarations. The generated lifecycle view separately shows the
current five-build observations; the older checkpoint below remains historical.

The fresh source-control capture executes 39 native witnesses in one test
group, 37 canonical raw VM observations, 74 original Rust observations and four
mutant observations. Its two Rust lifecycle groups and eight mutation groups
make 46 SDK submissions: ten accepted funding transactions, ten accepted CAT
transitions, 14 typed consensus rejections and 12 runtime umbrellas. Its 50
component VM observations remain separate. The two reports match byte for byte;
the installed packet passes 29 constructed corruption controls. These counts
are separate from the five-build main lifecycle above.

## What the lifecycle exercises

An actual standard-wallet genesis input funds an Eve CAT. Issuance executes the
selected zero-delta TAIL, followed by a transfer with real parent lineage and a
subsequent spend signed by the actual recipient. Each selected Rust mode and
source mutant funds its own reveal and committed asset ID. A single-coin ring
is a finite scenario, not evidence for every CAT ring or supply policy.

Invalid signatures are actual submitted bundles, rather than SDK signer
preflight failures. Intended consensus negatives require the exact pinned code
and full simulator equality in memory. GeneratorRuntimeError remains a runtime
umbrella and additionally requires the intended conclusive primary-VM failure;
it is not relabeled as a semantic consensus code.

The dropped-delta witness uses CAT input 1,000, CAT output 1,001 and an actual
separately signed one-mojo standard input. XCH conservation and CAT ring
accounting remain satisfied while the unchanged TAIL rejects the nonzero delta.
The separately committed dropped-delta TAIL permits it. The dropped-genesis
control likewise uses its own changed asset. Computing or discarding a guard
is not enforcement, and editing a JSON outcome is not a source mutation.

## Historical prepromotion checkpoint

The historical integrated-root prepromotion checkpoint passed on 2026-10-23, based on
commit `e953d8b06c0ce0776b1f67da2eb932686df1a121` plus the new experiment.
The ordinary package format, native tests and all-target Clippy gates passed,
as did 15 Python command-supervision and reader controls. Six complete schema-2
exports were freshly extracted and admitted, including both actual source
mutants in both optimization modes. Native, extraction and SDK source/tool
windows matched exactly. This checkpoint does not include a paired-corpus
promotion.

The two independently executed Rust lifecycle reports were byte-identical:
1,146,171 bytes, SHA-256
`3f44a90ebe014f76242d73bfbc951f457cbe4cb142e86dfb3d689b27894ab198`.
The machine-local capture is `target/cat-genesis-root-capture-a01`; its
`execution.json` and bounded command receipts retain the exact inputs and logs.
These host-bound files are intentionally outside version control. Earlier
sibling captures remain historical observations.

The original Rust module measured 2,157 bytes with optimization off and 735
bytes with optimization on. Their respective module tree hashes are
`b7fe66c5b17c00f49df3ab94c5b3663c55e2108fc050286a46c3e9174ad72328`
and `548b9ad1cb31d0ac0f623c99971ce2289abce58113a04576e94ded125f0aee69`.
Neither equals the canonical 45-byte TAIL identity above.

The executed Rust harness inventory is 39 native observations in one
native group; 37 canonical raw VM observations, 74 original Rust raw observations
and four mutant raw observations; two Rust lifecycle groups and eight source
control groups. The two modes compose 46 SDK submissions and 50 component VM
spend executions. Of 20 accepted transactions, ten are standard-wallet funding
and ten are CAT transitions. Fourteen are exact typed consensus rejections;
twelve are runtime umbrellas with separate primary-VM checks. Repeating the
report establishes local byte consistency, not additional scenario coverage.

The separate canonical-only root repeat also passed, including its 22 retained
reader corruption controls. Its two reports each contain 151,959 bytes with
SHA-256 `c0c13a9434720cfccf633ee8a5d2e03c838d8b6904d131fce4bae897023fcc80`,
retained beneath `target/cat-genesis-root-canonical-a01`. Across the two Rust
runs and two canonical runs, 138 actual SDK submissions executed; the repeats
add no distinct scenario coverage.

The canonical-only baseline is a separate 23-attempt protocol, with five
standard funding and five CAT transitions, seven typed consensus negatives,
six runtime umbrellas and 25 component VM runs. It retains its 50,000,000
primary component ceiling. Rust component replays use an explicit 250,000,000
ceiling; raw TAIL vectors use 1,000,000. The SDK transaction ceiling is
5,500,000,000 under the pinned Testnet11 block constant 11,000,000,000.
Primary VM clvmr 0.21.0 and SDK VM clvmr 0.16.2 are separate protocols. These are
resource limits, not interchangeable observed costs or a fee quote.

The bounded reader independently regenerates the 39 native expected rows and
checks their exact correspondence with native.json, actual native stdout, its
one-test summary and the bounded Cargo-command receipt. All 19 report
corruption controls and six compiled-codec controls passed on the current
capture. Retained hashes and
state-equality markers establish consistency, not authenticated execution,
a state serialization, a BLS theorem or compiler preservation.
