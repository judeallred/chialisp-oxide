# Iteration status

## Working and tested

**WIP prototype — not ready for real funds or production use.**

The latest local capture covers 23 annotated exports, including canonical
`p2-delegated-conditions` and a narrow optimization for helpers without explicit
`Cons` values. These helpers can still match borrowed `Tree::Pair` values.
Its repeated report records 459 named and 25,222 generated inputs. The earlier
[22-export composition report](../corpus/checkpoints/runtime-composition-validation.json),
`227ce911…`, remains an immutable checkpoint. Current proof, documentation and
publication gates have separate source and execution requirements; the local
capture does not establish production readiness or full Chialisp parity.

- Pinned rustc driver: complete analysis/borrow checking, typed MIR, strict
  checked arithmetic/branch semantics, explicit unsupported diagnostics, and
  compiler resource limits. The driver is compiler-private and host checked.
- An unpublished Cargo SDK and marker macro, multiple annotated exports, normal
  native checks/tests, and Cargo-driven CLVM extraction with real features and
  generated source. The paired corpus is the real `chialisp-oxide-puzzles` Cargo library;
  native execution, CLVM extraction, and proof replay use that same package.
- Nominal one-field transparent wrappers and typed condition helpers in the SDK
  crate; domain swaps fail Rust checking while
  wire representations stay explicit.
- Pure typed IR and direct Rue LIR generation, canonical untrusted ABI decoding,
  checked fixed-width bounds, byte arrays, tuples, booleans, and exact arity.
- An explicit SHA-256 bridge admits one reviewed safe native implementation by
  actual signature and complete typed-MIR closure/profile fingerprint. Cargo
  extraction and the local corpus capture cover twenty-three annotated exports,
  including the raw-domain `sha256tree` helper adapter, singleton launcher,
  original password tutorial module and canonical pay-to-singleton and
  delegated-conditions components.
  Immutable fixed-byte-array references coerce to slices for lengths up to 256;
  this does not admit general casts, mutable slices or arbitrary hash libraries.
- Ten original teaching examples, three exact canonical Intro expressions, and
  the canonical `p2_conditions` program factory, five pinned published helpers
  through explicit module adapters, the singleton launcher, original password
  module and canonical pay-to-singleton and delegated-conditions components, with
  side-by-side Chialisp/Rust/Rue sources and separately compiled Rue/Chialisp, the actual Rust functions as native
  oracles, 459 named and 25,222 generated integer/tree/byte vectors. Canonical closed
  expressions explicitly ignore environments; default typed ABIs remain strict.
- Forty complete-spend scenario groups run separately for Rust, Chialisp, and Rue
  (120 ledger scenario runs) through chia-wallet-sdk's simulator, plus
  standalone regressions for signatures/replay/messages/conservation/rollback.
- Hash-helper and pay-to-singleton comparisons use a recorded ten-million cost
  ceiling; the other twenty-one examples retain one million. Five deep hash inputs additionally run in all five
  builds at one million as 25 separate typed resource observations, outside
  passing coverage totals. The compatibility gate preserves all eighteen prior
  ports' artifacts and observations and passes 149 migration controls.
- The singleton launcher retains exact source/include/license/commit/tree
  provenance and its published 175-byte artifact. Independent Chialisp reproduces
  that identity; optimized Rust in the completed capture is 478 bytes with a
  different hash. Executed output and spend agreement is separate from module
  identity.
- Canonical pay-to-singleton preserves its raw environment and ignored tail.
  Its dedicated five-build composition submits 110 actual SDK transactions:
  15 are accepted and 95 match exact typed consensus rejections with complete
  in-memory simulator rollback. Independently reproduced canonical launcher,
  singleton top-layer and StandardLayer counterparts exercise Eve, the joint
  pay-to-singleton spend and one subsequent lineage spend. Only pay-to-singleton
  and the launcher are Rust source ports; this finite composition is not the
  entire singleton protocol. Three actual Rust policy mutations compile and
  have native/VM counterexamples. These transaction counts are separate from
  the forty generic ledger scenario groups.
- The exact password tutorial module preserves its raw environment and returns
  arbitrary condition trees. Separate fixed-hash curry instances record 48
  case groups in five builds, three varying-hash controls and 21 actual ledger
  submissions. Unsigned destination retargeting succeeds; currying does not
  repair that policy. Dedicated consensus rejections and broad runtime-umbrella
  observations have separate credit. The complete tutorial's funding/RPC/support
  workflow and a typed public instance-authoring API remain unimplemented.
- Canonical delegated conditions preserves two required raw projections and the
  ignored outer tail. `Cons<First, Rest>` owns its signature digest and borrows
  the unchanged conditions tree; tuples keep their proper-list encoding. Its
  dedicated suite records 200 native and 1,000 five-build VM observations,
  plus a separate 1,000 retained-byte replays over the same inputs. The ownership
  harness submits 60 actual SDK transactions: 20 accepted, including deliberate
  uncurried-key-selection demonstrations, and 40 exact typed rejections with full
  simulator rollback. A native test group with three policy observations and
  12 VM source-control runs detects compiling policy mistakes. This does not
  add a general dynamic-apply API or a canonical Rust module hash.
- Borrowed raw atom/pair trees preserve byte contents and improper tails.
  Optimized Rust reproduces the published factory module's bytes and hash;
  its actual IR rewrite has a separate structural-model equivalence check.
- Checked Rust self/mutual and non-tail recursive calls lower once per function
  into version-4 typed tables and strict lexical bindings. Ordered failure and
  divergence controls, fresh callee scopes and pre-lowering resource bounds are
  tested. Large acyclic helper graphs can use the same checked tables after a
  typed legacy expansion limit. Compile-time constants remain acyclic and
  cannot trigger this runtime fallback. Existing default proof models reject
  runtime IR; an explicit acyclic scalar model admits bounded nonrecursive
  scalar/tuple tables after ABI decoding.
- Ordinary Rust empty-slice patterns distinguish nil from nonempty zero-byte
  atoms. The published `last_inner` helper has pinned source/license/span and
  wrapper checks, proper/improper-list vectors and arbitrary returned trees.
- Opt-in `OwnedTree` adds restricted standard-Box pair construction and
  by-value recursive outputs. The published `prefix` adapter preserves arbitrary
  heads/tails; native whole-tree and result-depth limits remain oracle errors.
  Actual Cargo tests cover moves, borrows, snapshots, cleanup and failure order.
- Primitive immutable byte-slice and identical fixed-byte-array `==`/`!=` use
  actual resolved standard `PartialEq` identities, with byte-exact ABI/strictness
  tests and a pinned published inequality helper. General traits, structural
  tree equality and mixed slice/array comparison remain unsupported.
  Primitive immutable byte ordering now separately checks standard `PartialOrd`
  identities and uses unsigned lexicographic `>s`; the published insertion-sort
  helper combines this operation with owned recursive outputs. Ordered-byte
  model proofs remain unsupported.
- Lean algorithm models have 21 fixed theorem statements, kernel/axiom checks,
  corruption controls and a finite execution bridge. They do not establish
  Rust/MIR/IR/CLVM preservation; recursive IR remains outside the SMT models.
- Typed VM failures distinguish program rejection from resource/infrastructure
  failures. Ledger negatives require exact consensus codes; genuine incomplete
  and wrong signatures reach transaction validation instead of signer preflight.
- Source-derived SMT queries with independent specifications, nonvacuity,
  counterexample replay, mutation control, and fail-closed solver handling.
  The strict byte/Boolean gate extracts actual Cargo IR for the original
  inequality policy and five source controls; two further ABI controls are
  explicitly synthetic. All eight models retain off/on artifacts and selected
  VM replays; original-policy witnesses also use all five language builds.
  This whole-environment Seq8 contract is separate from earlier manual IR tests
  and does not prove the Rust/IR/CLVM translation. See the
  [byte proof guide](https://judeallred.github.io/chialisp-oxide/verification/byte-proofs/).
- Shared declared Cargo input/export provenance in validation and proof reports,
  checked against current local files before publishing documentation.
- Target-unit pinned sysroot/host-target checks, early backend-override rejection
  and bounded actual loaded-library checks cover core/alloc/std and primitive
  language/diagnostic owners, including aliases and dependency metadata.
  Native-valid relocation and response-routing controls preserve the separate
  trusted-installation assumption; these are not binary authentication.
- Seven Rue compiler regressions/fixes with pinned source provenance and upstream
  issues [108](https://github.com/xch-dev/rue/issues/108),
  [109](https://github.com/xch-dev/rue/issues/109),
  [110](https://github.com/xch-dev/rue/issues/110),
  [111](https://github.com/xch-dev/rue/issues/111),
  [112](https://github.com/xch-dev/rue/issues/112),
  [113](https://github.com/xch-dev/rue/issues/113), and
  [114](https://github.com/xch-dev/rue/issues/114).
- Searchable Astro/Starlight documentation in www, generated from source/evidence.
- Executable error studies: intentionally limited Chialisp tests miss concrete
  bugs rejected by native Rust type/name checking; a missing-cap negative
  control keeps policy mistakes and shared Rue guarantees explicit.
- Published-source discovery across 98 pinned repository trees and 3,582 selected
  files, with path/role/license provenance and offline tree reconstruction.
  This is source discovery, not 3,582 validated or distinct puzzles.
- An explicit functionality/operator map separates native expressibility,
  implemented compiler support, witnesses, activation rules, and missing proofs.

## Next compiler and puzzle work

Built-in fixed-width integer masks (`&`, `|`, `^`, `!`) and strict Boolean
`&`/`|`/`^` now lower actual MIR and have separate scalar SMT checks. They are
a prerequisite for broader algorithms, not general RustCrypto support. The
implemented [reviewed SHA bridge](hashing.md) hashes exact atom bytes and returns
32 bytes through strict version-4 runtime bindings. Installed rustc/std and
native-object consistency remain trusted; all proof models reject hash nodes.
The historical 22-puzzle composition passed 564 workspace tests and all-target
Clippy with warnings denied. Its retained report records 439 named and 22,983 generated
cases, repeated byte-for-byte, all 22 exports in both modes (44 export/mode combinations),
110 pay-to-singleton SDK submissions and fifteen conditions-helper measurements.
The reviewed authority is `corpus/runtime-composition-baseline.json` (`8024f743…`).
The source-only report `1a19cca4…` is an **unsigned reviewed content-addressed
checkpoint**; the unchanged source-cost transition anchors it to the actual
signed `e20f6d12…` checkpoint. The older guarded profile `66dc316c…` remains
historical pending metadata and is not an admission fallback.

The earlier hash gate's 168 corruption controls and 22,285 retained-artifact VM
replays remain historical results. Source-derived proof, error-study, Lean and
site results must retain their own source basis. The current 23-export capture
was executed twice with exact reports, fresh ON/OFF extraction in both repeats,
all 23 retained IR programs recompiled in both modes, and a separate five-build
replay of 1,000 VM evaluations over the same 200 inputs. The complete earlier
22-puzzle report and 44 IR/artifact combinations remain exact after the closed
source/native metadata transition.
Compiler verification carries the actual 578-test workspace and all-target
Clippy run for the same 327 Rust/Cargo/config inputs. Its sole added Rust file
is the non-workspace native-control template, tested separately. The final
integration reran the two Rue corpus tests on current data before rebuilding
the host tools; it did not rerun the entire 578-test suite. Proof and site gates retain separate results; these executions supply no proof
or publication credit.
These finite checks do not establish full Chialisp parity, compiler preservation
or production readiness.

The password module adds no compiler exception. It uses ordinary tree matches,
the reviewed hash helper and primitive byte equality. Its 21 instance submissions
are additional observations, separate from the 40 ordinary scenario groups.
The independent password checker checks exact source extraction, the complete
raw-tree relation, quoted curry trees, instance hashes, typed outcomes, state
rollback and finite evidence corruption controls. Retained-byte replay does not
resubmit consensus transactions or prove preservation. The completed checkpoint's
fresh extraction retains all twenty-two ports' raw IR in both modes. The current
append requires all twenty-three exports before any pure historical projection;
it never admits the old report as current evidence.

The cost iteration adds checked constant folding, forward execution-success
facts, total alias frame elision and exact constructor checks. All three closed
Intro expressions now match the original Chialisp bytes and 20-unit VM cost.
The signed 21-puzzle baseline and reviewed optimized outputs remain available
in `corpus/cost-optimization-baseline.json`; source IR and unoptimized artifacts
stay exact. This is finite regression evidence, not compiler preservation.

A second closed transition retains that signed report at commit `40579211` and
adds bounded helper inlining, entry-wrapper removal and unreachable-function
pruning. Strict argument bindings, alpha-renamed locals, recursive SCCs and the
runtime serialization boundary stay explicit. An optional candidate that hits
an existing compiler budget retries the preceding optimized IR through a typed
limit; other errors remain errors. The before/after rows live in
`corpus/helper-inlining-baseline.json` and its unchanged predecessor snapshot.
Rue codegen now uses worklists; the reproduced codegen cases pass on a 2 MiB
native thread stack, with a separate upstream regression and fix. This is not
a stack guarantee for every compiler phase.

The earlier password experiment exposed runtime overhead: optimized Rust was
407 bytes versus Chialisp's 31 and optimized Rue's 213. Its named valid curried
spend costs 7,208 VM units versus 1,321 and 3,937 respectively. Those finite
measurements include the instance wrapper but exclude bundle/condition costs.
The earlier guarded-only experiment reduced the module to 325 bytes; the completed
composition emits 271 bytes. The earlier curried-spend costs remain historical
measurements, not current results.
Any optimization must retain exact raw input domains and strict failure behavior;
narrowing the original ABI or discarding required work would change the program.

A third closed transition retains signed commit `c87957e2` and its unchanged
helper-inlining snapshot. Branch-local intervals now remove proven-total
repeated checked Add/Sub initializers without removing the final checked
operation. The integer decoder uses a lazy nil case and unsigned byte bounds
`[0080, ff80)`, preserving scalar shape, length, canonicality, numeric range and
body order. Compared with that signed milestone, optimized capped-payment is
1,275 → 539 bytes, split-payment 2,575 → 839, and counter 1,885 → 567. Eight of
247 accepted named inputs cost less in the primary VM, 239 are unchanged, and
none costs more in this finite set. All 21 unoptimized artifacts remain exact.
That historical transition remains in `corpus/runtime-refinement-baseline.json`.

A separate Seq8 model proves the signed-encoding law, with source-hash metadata,
positive/negative witnesses and model-source mutations. It does not translate
the decoder source or prove opcodes, integer-width admission, the complete ABI,
execution cost or compiler preservation. Finite decoder and interval tests
remain separate. Post-inlining refinement and larger recursive helper frames
still need profiling before further rewrites.

A constant-SHA experiment was **not adopted**. Folding the tiny `"abc"` hash
changed 22 → 47 module bytes while reducing primary VM cost from 699 → 151.
The isolated `p2_singleton` prototype grew 1,972 → 2,158 bytes while its 136
accepted inputs each saved 5,454 VM units. Complete transaction/generator costs
were not measured for that variant. A conservative size-aware version folding
only payloads of at least 32 bytes changed none of the then-published 21 ports or the
prototype. Its accounting for repeated traversal of shared LIR also needs work.
These external experiments add no published corpus or consensus coverage and
introduce no public compiler feature. Future optimization should compare both
serialized programs and actual transaction costs rather than VM cost alone.

1. Extend the implemented runtime-call IR and checked Rust recursion to further
   published list helpers. `last_inner` now has empty-atom matching and an
   explicit empty/improper-list contract through a visible module adapter.
   The owned `prefix` adapter now exercises a genuine owned result; further
   helpers, byte storage and collection operations remain work. Preserve strict binding/order,
   pre-lowering resource bounds and current model/optimizer exclusions; see
   [runtime calls](runtime-calls.md).
   Continue reducing measured runtime overhead without moving failures past
   divergence or duplicating evaluation. Frame elision reduced the optimized
   `last_inner` adaptation from 743 to 369 bytes; the Chialisp helper adapter is
   71 bytes and Rue is 125. On `(1 2 3)`, optimized Rust cost fell from 25,933
   to 11,568, compared with Chialisp's 2,236 and Rue's 3,814. These are pinned
   measurements checked by `scripts/check-runtime-frames.py`, not general
   performance guarantees. Unused total discriminant bindings and broader
   sharing remain opportunities; first establish totality with bounded analysis.
2. Expand the Cargo SDK beyond transparent domain wrappers to typed
   structures/enums, immutable lists,
   further reviewed primitives, currying, and carefully scoped library support.
   `OwnedTree::Atom` still borrows immutable bytes and cannot return an escaping
   borrow of a newly computed local digest. The reviewed `Cons<First, Rest>`
   result instead owns a digest in its head while borrowing the unchanged tree
   tail. This explicit v5 pair supports the locally validated delegated-conditions
   port. Tuples retain their proper-list
   encoding. General owned atom storage and arbitrary structs still need their
   own native and CLVM semantics.
   The optimized delegated-conditions module is 629 bytes (Rust off: 1,534),
   compared with canonical Chialisp's 137 and Rue's 293 off / 251 on. The current
   helper optimization accepts only signatures and bodies without the explicit
   `Cons` wire type, with calls restricted to self-calls. Matching borrowed
   `Tree::Pair` values remains supported. Within Pair-capable programs,
   constructors, projections and entry bodies keep their strict lowering.
   Broader sharing and Pair-aware optimization still need explicit
   semantic and resource controls. None of these sizes is a full transaction-cost
   comparison or module-identity claim.
   The pinned `p2_singleton` module now has actual Cargo admission, raw-domain
   differential cases, instance hashes and real announcement composition tests.
   Its fixed three-condition result owns digest arrays. Source hash sharing
   reduced optimized Rust from 1,972 to 1,685 bytes versus canonical Chialisp's
   403. In the recorded joint bundle, total conditions cost fell 7.06% to
   45,309,974 while ordinary VM cost increased by 1,254. Those are the historical
   source-only measurements. The completed guarded/Atom/unary composition combines
   that exact policy source with the reviewed emitter: 1,334 bytes, 57,925 ordinary
   VM cost and 41,090,749 measured joint cost. Its seven smaller optimized modules
   have no measured accepted-case cost regressions; all 22 unoptimized and 66
   independent-language artifacts remain exact. This does not establish universal
   cost dominance.
   Judge changes by actual serialized generator bytes and full conditions
   costs as well as standalone VM cost.
   The legacy totality cache still treats slice-pattern metadata conservatively,
   so an inlined atom helper can retain a redundant evaluation sequence. Any
   refinement must consult the validated pattern plan rather than declaring
   arbitrary pointer/length metadata total.
3. Migrate actual canonical tutorials and CHIP puzzle implementations, recording
   exact source/license/revision, module identity constraints, behavior, and
   integration compatibility. The current original teaching examples do not
   count as completed CHIP ports; the page/CHIP inventory has 98 source entries.
   The separate example inventory records exact pinned primer/concept snippets
   and workflow gaps. Three closed Intro expressions have finite differential
   validation with their original environment behavior preserved. Their parent
   page remains partial; the other nine programs, variant, and supporting
   workflows remain unrun. Next ports need explicit semantic contracts before
   expanding parameter, list, recursion, cryptographic, or evaluation support.
   The singleton launcher source port does not complete the singleton protocol;
   broader continuation/lineage, curried-instance compatibility and wallet
   integration require their own witnesses and adversarial complete-spend tests.
4. Extend the implemented source-derived strict byte-argument/Boolean model
   beyond the current exact-inequality contract to broader byte operations,
   cryptographic assumptions, state transitions and compiler preservation.
   SHA remains explicitly outside current models: do not substitute an
   uninterpreted or collision-free function and call that a cryptographic proof.
   Preserve independent whole-environment specifications, nonvacuity and actual
   source-control/native/retained-artifact replays. Preserve the enforced
   [pinned-toolchain boundary](toolchain-trust.md) for custom sysroots and
   loaded replacement standard libraries while extending primitive support.
   Installed toolchain integrity, broader plugin configuration and build inputs
   remain trusted; authenticated complete-build provenance remains open. Connect the
   implemented Lean algorithm models to source-derived semantics rather than treating a manual
   model and finite execution bridge as a compiler theorem.
5. Add larger adversarial fuzzing, upgrade/fork matrices, and independent audits.

## Hosting

The owner made the repository public and enabled Pages. Use GitHub Actions as the
Pages source. `.github/workflows/pages.yml` builds www with pnpm and deploys
www/dist to https://judeallred.github.io/chialisp-oxide/. Verify the actual
successful deployment before reporting the site as live.
