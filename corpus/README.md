# Source and migration inventory

The validated teaching corpus is the real `chialisp-oxide-puzzles` Cargo library under
[`puzzles`](../puzzles). [`puzzles/authoring.json`](../puzzles/authoring.json)
maps each paired source to its annotated export and declares local Cargo inputs.
The validation and proof reports record their hashes. Native execution links
that library; CLVM and proof IR are extracted through Cargo from the same source.
Run `./scripts/chialisp-oxide validate` and `python3 scripts/check-ir-proofs.py` after
building the workspace. The report separates ten original teaching fixtures,
three canonical Intro expression ports and one canonical program factory. Only the Intro ports count toward
their exact example-level tutorial records; no CHIP port is complete.

The original `rightmost-tree` example compiles checked Rust recursion through a
finite version-4 function table. Native Rust and five independently built CLVM
programs compare 13 named and 2,070 generated raw-tree cases, including returned
atoms up to 8,192 bytes and a right spine of depth 128. The host oracle rejects
deeper comparison requests as errors, not policy rejection; the compiled helper
has no depth guard. This pure function has no ledger or model-proof claim.

`inventory.json` records 98 source entries as of 2026-10-16: all 28 Markdown
documentation pages in the pinned Chialisp site, all 64 numbered CHIP index
entries, five grandfathered/obsolete standards, and the auction proposal listed
under consideration. This is a source inventory, not 98 implemented puzzles.

The eight tutorial pages cover the primer and concepts. The remaining Chialisp
pages cover language/VM references, costs, conditions, errors, attacks,
optimization, debugging, and seven protocol primitives. The original password
coin's insecurity must remain explicit when the lesson is migrated.

CHIP status is copied from the versioned index, including draft, review,
stagnant, withdrawn, obsolete, and grandfathered material. A proposal's status
does not imply that it contains a puzzle. Every entry still needs reference
discovery or review: links ending in `.clsp`, `.clvm`, or `.rue` were indexed as
leads, including upstream dependencies. They have not been imported or verified
as the definitive complete implementation. Branch links remain visibly
unpinned; their licenses are unset until inspected. The proposal/doc license
does not establish a linked implementation's license.

Source snapshots:

- [Chialisp documentation](https://github.com/Chia-Network/chialisp-web/tree/dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8/docs)
- [CHIP index](https://github.com/Chia-Network/chips/blob/6fa7625dfcc895177dbecac20c838498e6a60ceb/README.md)
- [Consensus condition opcodes](https://github.com/Chia-Network/chia_rs/blob/56f6b676a96de8659fe05d7e4f705df553f37fa5/crates/chia-consensus/src/opcodes.rs)

An original small fixture that tests a similar idea does not count as migrating
the corresponding tutorial or protocol. To promote an entry, identify each
example or module, inspect and pin the canonical source and license, add paired
local source paths, and attach reproducible differential/simulator evidence.
Reports must distinguish tested behavior from module-hash compatibility and
list remaining assumptions. A protocol that commits to an original module hash
cannot substitute behaviorally equivalent bytecode without further work.

Run `python3 corpus/validate_inventory.py`. The validator rejects unpinned
source snapshots, duplicate entries, broken evidence links, and validation
claims without implemented local sources and evidence. It does not verify
equivalence by itself; the compiler/VM/simulator suites generate that evidence.
The Intro page is `in_progress` / `not_run`: its three expression records have
finite differential evidence, while whole-page validation remains incomplete.
Other page/CHIP entries remain planned and unvalidated.

## First canonical expression ports

`tutorial-examples.json` enumerates twelve distinct programs, one naming variant,
and supporting workflows in the pinned tutorial snapshot. The first three
Intro expressions are `implemented` / `differential_only`: `(list 1 2 3)`,
`(+ 2 3)`, and `(* (if (> 3 2) 10 5) 10)`. Their local reference files extract
only the original expression from the single-quoted `run` command and append a
newline. No `mod` wrapper or argument guard is added.

The corresponding Cargo exports explicitly declare
`#[chialisp_oxide::puzzle(environment = "ignored")]`, restricted to parameterless
functions. This retains the intended ignored-environment contract instead of
requiring the strict nullary `()` ABI. Every expression has eight named input
trees and 256 generated trees compared across native Rust, Rust CLVM in both
optimizer modes, and independently compiled Chialisp. These are pure language
results, not ledger spends. Differing tree hashes/costs remain visible.

Each promoted inventory record binds its original span, extracted reference,
Rust source, manifest, and report by SHA-256, plus the explicit environment
contract and observed output. Run `python3 corpus/validate_tutorial_examples.py`
to check those bindings and `--self-test` to exercise rejection controls. Nine
other programs, the naming variant, and supporting workflows remain unrun;
partial Intro progress does not complete its parent page or the primer.

## Three-language comparisons and discovery

Every current fixture records Chialisp, Rust, and independently compiled Rue
source. Five CLVM builds cover Chialisp, Rust off/on, and Rue off/on. The same
31 ledger scenario groups run separately for each language: 93 scenario runs,
including 69 exact consensus rejection observations. Groups can submit several
bundles, so scenario runs are not transaction counts. Rue source hashes and
local dependencies are recorded separately from Rust's Cargo authoring inputs.
The `add-rue` baseline comparison preserves every preexisting Rust/Chialisp
artifact, cost, outcome and ledger result while admitting only the added Rue
evidence.

`published-puzzles.json` and `validate_published_puzzles.py` catalog concrete
source files in pinned public repository trees. Its discovery query snapshot,
classifications, license metadata and omissions are explicit. This catalog does
not mark sources as ported or verified. `error-study.json` is independent
executed evidence for the deliberately faulty fixtures in
`experiments/error-study`; reproduce it with `python3 scripts/check-error-study.py`.
