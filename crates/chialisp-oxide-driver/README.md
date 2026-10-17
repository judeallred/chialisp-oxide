# Rust frontend

`chialisp-oxide-driver` uses compiler-private APIs from **nightly-2026-07-14**
(`rustc 1.99.0-nightly`, commit `daf2e5e18`), with `rustc-dev` and
`llvm-tools-preview`. The workspace pins that toolchain; no `RUSTC_BOOTSTRAP` is
used. Updating it requires reviewing rustc's API and rerunning the semantic and
compile-fail corpus. The executable embeds its build compiler's sysroot and
checks Cargo's compiler commit before loading compiler-private metadata.

The user workflow is `cargo chialisp-oxide build` with a dependency on `chialisp-oxide` and
`#[chialisp_oxide::puzzle]` on each exported free function. Cargo owns dependency
resolution, features, build scripts, procedural macros, and generated source.
The annotation macro validates function syntax and adds an export marker; it
preserves the original Rust function tokens and performs no translation.

Exports use a strict, complete typed argument-list ABI by default, including
validation of unused parameters. An explicit
`#[chialisp_oxide::puzzle(environment = "ignored")]` is permitted only on a
function with no parameters, including no unit or underscore parameters. It
accepts any CLVM environment tree without decoding it; the Rust function has
no access to that environment. `environment = "strict"` makes the default
explicit. `environment = "raw"` instead requires exactly one logical tree
parameter, ordinarily `&'a chialisp_oxide::Tree<'a>`, and passes the entire
environment to that parameter. Atom bytes and improper pairs are preserved.
It does not grant arbitrary evaluation or CLVM operator intrinsics.
The policy changes only input decoding: assertions, discarded
failing calculations, and lazy branches retain their usual semantics.

Strict exports retain the `__oxide_puzzle_v1` metadata marker. Ignored exports
use `__chialisp_oxide_puzzle_v2_environment_ignored`; raw exports use
`__chialisp_oxide_puzzle_v3_environment_raw`. These markers describe the input
policy, independently of runtime calls. Runtime-free strict/ignored/raw programs
retain their legacy/version-2/version-3 JSON respectively; programs with runtime
nodes or a function table use a version-4 envelope with the same input policy.
Older readers cannot silently treat a versioned envelope as a strict program.
The driver checks all markers together, rejects duplicates and
conflicting or unknown reserved markers, and independently enforces the
nullary or single-tree restriction after Rust checking.

Internally, the CLI selects a library package, invokes Cargo with an explicit
host `--target`, and sets `RUSTC_WRAPPER=chialisp-oxide-driver`, `CHIALISP_OXIDE_CARGO_MODE=1`,
`CHIALISP_OXIDE_PRIMARY_PACKAGES` (comma-separated crate names), and `CHIALISP_OXIDE_OUTPUT_DIR`.
The wrapper forwards compiler probes and host build-script/proc-macro units.
Target dependencies retain MIR for supported cross-crate extraction. Selected
packages undergo complete Rust analysis and emit one validated JSON program per
annotated entry; successful compilation continues to produce normal Cargo
metadata. Export filenames hash complete paths to distinguish `a__b` from
`a::b`. The CLI publishes bundles only after the entire Cargo command succeeds.
Existing compiler wrappers are currently rejected by the CLI rather than
composed. The SDK crate is the only permitted external source of lowered
functions or wrappers; a crate name is not a package authentication boundary.

The paired corpus is the `chialisp-oxide-puzzles` Cargo library. Its annotated
modules export paths such as `chialisp_oxide_puzzles::bounded_counter::puzzle`; the
native oracle calls those same Rust functions. Corpus validation builds the
library once through Cargo and loads the typed exports from the fresh bundle.

Target configuration must use the pinned canonical sysroot and built-in host
target. Common extraction/metadata callbacks check parsed configuration, then
bound and inspect the frozen loaded-crate graph before exporting IR or
continuing target metadata. Actual core/alloc/std and primitive language/
diagnostic item owners must resolve directly inside the pinned target-library
directory, including aliases and transitive dependencies. Explicit codegen
backend overrides reject before target probes that can load a backend early.
Native-valid copied metadata, sysroot aliases, response routing and probe
controls live in `tests/toolchain.rs`.

The router expands a copy for classification but passes original argv to rustc;
parsed crate/target identity must match. Response-file contents and installed
toolchain bytes remain trusted mutable build inputs. This is location-policy
enforcement, not authentication or a compiler/plugin sandbox. See
[toolchain trust](../../docs/toolchain-trust.md).

A standalone source harness remains available for independent compiler
regressions and research:

```sh
cargo build -p chialisp-oxide-driver
target/debug/chialisp-oxide-driver crates/chialisp-oxide-driver/tests/fixtures/guarded_payment.rs --output program.json
```

That mode compiles an edition-2024 library and selects its unique free function
named `puzzle` (`--entry NAME` changes the name). Both modes run rustc's complete
analysis, explicitly force borrow checking for every local type-check root
(including its nested closures), and check diagnostics before lowering typed
MIR. The frontend does not parse printed MIR or translate Rust syntax with
`syn`. A Rust checking or lowering failure prevents a successful export.
The standalone source harness rejects `@response-file` input; use Cargo for
compiler response files, or spell a literal leading-`@` filename as `./@name.rs`.

The supported subset includes:

- `bool`; `u8` through `u128`; `i8` through `i128`; unit; tuples and fixed arrays
  of at most 256 items. Byte arrays have a distinct byte-string ABI.
- Local or SDK, non-generic `#[repr(transparent)]` structs with exactly one
  field, no custom `Drop`, a nonzero layout, and a recursively supported inner
  type. Rustc checks nominal distinctions such as `PuzzleHash` versus `CoinId`
  before wrapper construction and projection erase to the inner ABI.
- Local values, tuple fields, comparisons, boolean/integer `!`, `if`,
  integer/boolean `match`, and acyclic control flow.
- Built-in `&`, `|` and `^` on matching fixed-width integers or `bool`.
  Integer complement preserves its Rust width: unsigned `!` xors with that
  width's maximum; signed `!` xors with -1. Boolean `&` and `|` evaluate both
  operands, unlike `&&` and `||`. User trait names do not grant primitive semantics;
  local supported implementations retain their actual MIR bodies. Shifts and
  explicit wrapping operations remain unsupported.
- Immutable borrowed trees: exactly two enum variants with fields `(&[u8])`
  and `(&Self, &Self)`, one lifetime, ordinary Rust enum representation, and no
  destructor. This validates structure rather than names or discriminant order.
  Local and SDK declarations are accepted; macro-generated declarations are
  rejected. Shared reborrows, variant matching, projections, constructors and
  lifetime-only helper functions erase to explicit typed tree operations.
  `&[u8]` represents an arbitrary atom, distinct from a fixed byte array. Static
  atom/tree constants lower their inspectable, bounded promoted/CTFE MIR rather
  than assuming a Rust pointer layout. Immutable `&[u8; N]` to `&[u8]`
  coercions are supported for N <= 256, preserving exact bytes and the source
  value captured before later reassignment. Mutable/general pointer coercions
  remain unsupported.
- SDK `sha256(&[u8])` and `sha256_parts(&[u8], &[u8], &[u8])` return
  the exact 32-byte SHA-256 of the input bytes/concatenation. The wrapper follows
  ordinary MIR. The reserved public multipart entry is authorized only by its
  exact safe signature and a SHA-256 fingerprint of its complete, exhaustively
  encoded typed-MIR closure, constants, concrete layouts, codegen attributes and
  target/profile. Names, package versions, source paths and caller markers never
  authorize the operation. Modified reserved entries fail closed. The reviewed
  profiles are the pinned compiler's checked/debug-asserting, panic-unwind Cargo
  builds on aarch64 macOS and x86_64 Linux. Every source hash forces strict runtime
  bindings, including unused results; resource failures stay inconclusive. This
  bridge trusts the reviewed native kernel, installed compiler/standard library
  and metadata/native-object consistency. It is not a cryptographic correctness
  or compiler-preservation proof, and does not add general hashing libraries,
  slice indexing, lengths, dynamic allocation or arbitrary external calls.
- Empty-slice patterns on the accepted immutable atom slice, including
  `Tree::Atom([])`. The frontend validates the complete compiler-generated
  metadata/zero-comparison use chain, including an optional fake pointer,
  and lowers only an atom capture
  and boolean empty check. Internal pointer/`usize` temporaries cannot escape
  that chain. Nonempty zero bytes are not nil; general slice lengths, indexing,
  raw pointers and positive-length patterns remain outside this support.
- Checked `+`, `-`, `*`, with overflow becoming puzzle rejection. Overflow
  checking and debug assertions are always enabled, independently of Cargo's
  profile. Explicit wrapping/overflowing methods are rejected.
- Plain `assert!` as puzzle rejection, and local or SDK helpers without type or
  const generics. Acyclic source graphs inline into expressions; reachable
  recursive graphs use a closed runtime function table. Strict bindings retain
  unused calls and evaluate their arguments once, left to right. Self, mutual,
  and non-tail recursion are supported within the accepted types/MIR subset.
- Byte arrays passed as parameters and constant byte-array construction.
  Primitive byte equality and unsigned lexicographic ordering use ordinary
  `==`, `!=`, `<`, `<=`, `>` and `>=` on immutable `u8` slices or identical
  fixed byte arrays. Actual core `PartialEq`/`PartialOrd` methods and concrete
  implementations are checked before erasing types; custom implementations,
  mutable operands and mixed slice/array comparisons remain unsupported.
  Named fixed-byte constants, including supported transparent wrappers, are
  evaluated by rustc and read only after checking their exact bounded layout,
  allocation bounds, initialized bytes, and absence of pointer provenance.
  Tuple fields and MIR constant array projections are supported; ordinary Rust
  indexing is rejected when it requires an index local or reference.

Unannotated native functions may use ordinary Rust features, including
allocation and closures, provided they are not reachable from a puzzle. They
still undergo Rust checking. General references, type/const generics, other external calls,
CFG loops, general allocation, ordinary structs/enums, casts, floats, shifts,
division/remainder, and non-Rust function ABIs are rejected
when encountered in an exported puzzle. Panic messages and formatted
assertions currently require unsupported Rust formatting calls. Unsafe Rust and
unstable source features are forbidden across the selected library and SDK by
rustc's `-Funsafe-code` and `-Funstable-features`, including code that MIR would
otherwise eliminate. Dependency MIR must be available; missing metadata gets a
diagnostic rather than an unchecked assumption. Each unsupported MIR diagnostic
includes its Rust source location.

In the legacy acyclic route, strict failure effects are retained at their
defining MIR operations. Copies, aggregates, and intrinsically total helpers pass previously computed values
without introducing another failure sequence. A conservative analysis runs
only after successful lowering; arithmetic, assertions, divergence, and unknown
operations remain fallible. Its cache holds at most 256 functions, with at most
1,024 blocks and 8,192 statements inspected per function. Missing or excessive
cases keep their evaluation effects.

Legacy helper inlining is limited to 32 nested calls. Source expansion allows
64 active control-flow blocks, 10,000 total visited blocks, 8,192 nodes per
expression, expression depth 64,
and 500,000 cumulative expression nodes of lowering work. Argument/environment
cloning and aggregate construction have separate size checks. Type expansion
is limited to 4,096 nodes and depth 32 before replicating nested arrays.
Functions have at most 64 parameters and 2,048 MIR locals. Accepting a function
does not establish an affordable CLVM execution cost.
The initial MIR scan rejects bodies above 8,192 statements or 10,000 blocks.
Empty-slice metadata analysis also bounds collected uses, alias propagation,
and zero-definition scans to 500,000 work units, with at most 64 nested zero aliases.

## Runtime function extraction

`runtime_functions` discovers a finite graph of actual Rust `DefId`s, validating
supported signatures and calls before selecting the lowering route. Acyclic
graphs retain legacy expression extraction. A graph containing recursion becomes
a dense function table, with the source entry at ID zero and each body lowered
once in its own argument scope. Shared tree/byte references retain their checked
Rust lifetime/ownership boundary; no mutable references or closure captures are
introduced. Supported constants/promoted bodies use the separate legacy
acyclic lowering path. Runtime projected assignments are explicitly rejected.

Non-atomic MIR definitions and call results become typed lexical `Let`s.
Previously bound arguments/locals and primitive literals may be forwarded
without a new binding. This never elides a defining call or checked projection.
The backend evaluates strict operands once, left to right, through quoted
continuations; each call receives fresh slots and the compiler-owned table.
`If` remains lazy. Runtime recursion does not expand the compiler's call stack
or require a new puzzle attribute.

Discovery allows at most 256 functions/10,000 visited graph blocks; a runtime
table has at most 64 functions. Each runtime function has at most 2,048 generated
lexical bindings and retains the source expression/CFG limits above. Backend
lowering additionally bounds new frame bindings to 128 beyond arguments and
charges copied lexical types against 500,000 work units before cloning. Later
codegen has its own estimated four-million-unit/depth-96 expansion check; these
limits can reject a structurally valid table before emitting CLVM.

Durable source/native/VM tests are in `tests/runtime_calls.rs`; direct IR
ordering, frame, ABI, and budget tests are in the IR crate's
`tests/runtime_evaluation.rs` and `tests/runtime_validation.rs`. Intentional
nontermination runs only inside CLVM with a finite cost limit. Cost, stack, and
allocator failures are resource/inconclusive outcomes, not policy rejection.
Current SMT models reject all runtime IR, including unused tables and dead
calls; finite replay does not prove termination or compiler preservation. See
[`docs/runtime-calls.md`](../../docs/runtime-calls.md).
Empty-atom source patterns and rejected metadata escapes have their own Cargo
regressions in `tests/empty_atoms.rs`. The paired `last-list-element` corpus
entry ports a pinned published helper through an explicit Chialisp module
adapter; it does not assert whole-library or canonical module-hash compatibility.

This is host-checked Rust, not a rustc CLVM target. Source expansion uses the
pinned compiler's host configuration, including `cfg(target_*)`,
`cfg(debug_assertions)`, pointer size in compile-time expressions, and standard
library availability. Avoid target-dependent source in reproducible puzzles;
`usize`/`isize` are rejected in the accepted ABI. The supported subset does not
imply that ordinary Rust libraries can run on CLVM.

The output is the typed `chialisp-oxide-ir` JSON contract. The backend validates it
again and checks untrusted solution encodings before evaluating the puzzle.
Transparent wrappers carry source-level nominal identity, not wire tags:
`PuzzleHash([u8; 32])` and `CoinId([u8; 32])` both decode as exactly 32 bytes.
Decoding does not invoke constructors or establish user-defined invariants;
puzzle checks must enforce these at runtime. Zero-sized wrappers, multiple
fields (including extra zero-sized fields), generic wrappers, other external
wrappers such as `NonZeroU64`, destructors, and macro-generated wrapper
declarations are rejected. Apart from the narrowly validated borrowed and owned trees,
named tuple/other aggregate constants and borrowed
wrapper methods remain unsupported. The SDK's `Copy` domain values provide
by-value constructors, unit-explicit getters, and comparisons without adding
reference or trait-operator semantics to the compiler.

Borrowed trees support traversal; opt-in `OwnedTree` additionally supports
restricted owned pair construction and recursive accumulators. Actual standard
Box/Global/constructor identities and closed compiler metadata chains are
validated; pointer values remain separate from logical locals. Native move and
borrow checking precedes extraction. Custom Drop/allocators, user pointers and
general Box APIs remain unsupported. Explicit global allocators visible in the
crate graph reject owned exports. The real Cargo regressions in
`tests/owned_trees.rs` check snapshots, cleanup, strict failure order and
ownership diagnostics. See [owned trees](../../docs/owned-trees.md). This does
not provide unbounded integer arithmetic, hashing, arbitrary application,
dynamic atom storage, or general collections.
Source and value equivalence alone do not establish byte/hash/cost identity
with a canonical module. The optimized `p2_conditions` Rust port now reproduces
the published nine-byte module and tree hash; its unoptimized artifact differs.
For runtime-free programs, the backend first applies a conservative typed IR
pair-guard pass, then Rue LIR optimization. The owned guard pass skips runtime
programs because successful-evaluation facts do not establish prompt rejection
before divergence; Rue LIR optimization still runs in the optimized mode. Checked projections retain atom rejection when a redundant guard
is removed. This measured identity for one factory does not establish general
module compatibility or compiler preservation.
