---
title: Supported Rust
description: The implemented subset, rejected features, arithmetic behavior, and host-configuration limits.
---

Cargo supplies the selected library's actual edition, features, dependencies,
and generated source. Mark each exported free function with
`#[chialisp_oxide::puzzle]`; several marked functions can coexist without a special
function name. The annotation preserves the function body.
For closed expressions that must preserve an unrestricted original environment,
parameterless exports can explicitly opt into
`#[chialisp_oxide::puzzle(environment = "ignored")]`. Parameterized functions
cannot use this option. Ordinary annotations keep the strict typed argument
ABI; see [ignored environments](/chialisp-oxide/reference/abi/#explicitly-ignored-environments).

Extraction uses `nightly-2026-07-14`, reporting Rust 1.99.0-nightly. Ordinary Rust
analysis, including type and borrow checking, must succeed before typed MIR
lowering. No `RUSTC_BOOTSTRAP` is required. Printed MIR is not parsed. The
installed pinned compiler and matching unmodified standard library are trusted.
Target configuration and actual loaded core/alloc/std and primitive-owner
locations must use the pinned installation. Canonical aliases work; relocated
roots, custom targets and backend overrides reject. These checks do not
authenticate installed bytes. See
[compiler trust](/chialisp-oxide/reference/compiler-trust/) and the
[toolchain trust notes](https://github.com/judeallred/chialisp-oxide/blob/main/docs/toolchain-trust.md).

## Implemented values and control flow

- `bool`, `u8`–`u128`, `i8`–`i128`, unit, tuples, and fixed arrays of up to 256 items.
- Bounded standard `Option<T>` outputs: `None` encodes nil and `Some(value)` a
  one-element proper list. Construction, whole-value assignment and opaque
  monomorphic helper passing are supported, including optional `Cons` tails.
  Version-8 exports also accept optional arguments and ordinary by-value
  `match`/`if let` inspection, with exact nil/singleton and complete payload
  validation. Older v6/v7 entry contracts stay output-only. Generic methods,
  references to `Option` and owned/drop payloads remain rejected. This is a closed
  version-6 capability, not general enum support. See
  [optional output lists](/chialisp-oxide/reference/optional-lists/).
- Local values, tuple fields, comparisons, boolean negation, `if`, integer and
  boolean `match`, and acyclic control flow.
- Checked `+`, `-`, `*`, `/` and `%`, plus width-preserving integer shifts;
  overflow, zero divisors and invalid shift counts reject the puzzle.
- Built-in integer `&`, `|`, `^` and `!` at each supported signed/unsigned
  width; `bool` also supports strict `&`, `|` and `^`. These do not add
  wrapping arithmetic. Checked integer shifts are supported with explicit count guards. See [bit masks](#bit-masks-and-evaluation-order).
- Primitive integer `as` casts between all ten fixed-width signed/unsigned
  types (`i8`–`i128`, `u8`–`u128`). Narrowing truncates bits rather than rejecting
  overflow; widening and signedness changes follow Rust. See
  [integer conversions](#integer-casts-preserve-rust-conversion-rules).
- Plain `assert!` as puzzle rejection. Formatted assertions and panic messages
  are rejected because their formatting calls are outside the current subset.
- Local/SDK helpers, including lifetime-only generics for borrowed and owned trees. Acyclic
  source graphs inline into expressions; self, mutual, and non-tail recursion
  use a closed runtime function table. Type/const generic functions remain rejected.
- Named scalar/fixed-byte constants and bounded nested tuple, array, `Cons`, named-record
  and transparent-wrapper constants: policy terms, typed amounts, hashes and public
  fixture keys. See [named aggregate constants](#named-aggregate-constants).
  The separate restricted tree route also lowers supported promoted shared
  constants from their real MIR; it does not grant general pointer constants.
- Byte-array parameters and constant byte arrays, with a byte-string ABI.
- Ordinary `==` and `!=` on immutable `u8` slices and identical `[u8; N]`
  arrays through immutable references, within the existing 256-element limit.
  Actual primitive standard `PartialEq` implementations are resolved before
  logical type erasure. Mixed slice/array comparisons, custom trait
  implementations and arbitrary tree equality remain unsupported. See
  [exact atom bytes](/chialisp-oxide/tutorials/atom-not-equal/).
- Ordinary `<`, `<=`, `>` and `>=` on those same primitive byte types use
  unsigned lexicographic ordering, matching CLVM `>s`. Proper prefixes come
  first; padding and high-bit bytes remain data. The actual standard
  `PartialOrd` members and concrete implementations are checked separately
  from equality. This adds neither numeric decoding nor custom trait dispatch.
  See [order entries by byte keys](/chialisp-oxide/tutorials/insertion-sort-by-keys/).
- Nongeneric, single-field `#[repr(transparent)]` nominal wrappers declared
  locally or in the vetted SDK, over supported nonzero-size values.
  Rust checks their identity; lowering erases
  them to the inner ABI type. See [typed puzzle helpers](/chialisp-oxide/tutorials/typed-sdk/).
- Local nongeneric named structs with bounded supported value fields, using
  declaration-order tuple ABI. Construction, by-value passing, moves and bounded
  record-root field updates retain strict snapshots. See [named value records](#named-value-records).
- Vetted by-value SDK methods such as `Amount::from_mojos`, `.mojos()`,
  `.is_at_most()`, `Timestamp::from_unix_seconds`, and `.unix_seconds()`.
  The SDK's small transparent values support implicit `Copy` reuse. Explicit
  `.clone()` calls remain unsupported because their borrowed receiver and trait
  dispatch are outside the accepted subset.
- Fixed-array construction, repeats and literal-index reads such as `values[0]`.
  Byte arrays keep their exact atom ABI; other arrays use index-order proper
  lists. See [fixed arrays and literal indexing](#fixed-arrays-and-literal-indexing).
- Empty-slice patterns on supported atom slices, such as `Tree::Atom([])` or
  `OwnedTree::Atom([])`.
  Nil matches; nonempty zero bytes do not. The frontend validates the complete
  pinned MIR metadata-test chain and erases its compiler-generated pointer and
  `usize` intermediates. General slice lengths/indexing and raw pointers
  remain rejected. See the [published list helper](/chialisp-oxide/tutorials/last-list-element/).

Byte arrays and tuple-like products use different encodings. The compiler does
not use Rust's native in-memory layout as an on-chain ABI.

The [byte-policy proof guide](/chialisp-oxide/verification/byte-proofs/) checks
the actual Cargo IR of one strict byte-argument/Boolean export against an
independent whole-environment specification. It covers the original, five real
source controls and two labeled synthetic ABI edits, with nonvacuity checks and
selected native/retained-program replays. This model result is separate from
frontend support tests: it does not prove standard-library bridging, the
emitted decoder, complete Rust/Chialisp parity or compiler preservation.

## Bit masks and evaluation order

Use named constants to explain each bit's meaning. A mask can express which
features a policy allows; it does not establish who may exercise them.

```rust
const READ: u8 = 0b0000_0001;
const WRITE: u8 = 0b0000_0010;
const KNOWN: u8 = READ | WRITE;

#[chialisp_oxide::puzzle]
pub fn validate_flags(flags: u8) -> bool {
    assert!((flags & !KNOWN) == 0);
    (flags & WRITE) != 0
}
```

Here `!KNOWN` is the 8-bit value `0b1111_1100`. It is not CLVM's unbounded
complement, which would be negative. The frontend lowers unsigned complement
as XOR with the maximum of the actual Rust width. Signed complement uses XOR
with -1; signed values retain two's-complement semantics. Bitwise results stay
within the original type, while the exact numeric ABI still rejects padded,
out-of-range and incorrectly signed inputs. This is integer arithmetic, not a
byte-string comparison or operation on serialized CLVM.

On Boolean values, `left & right` and `left | right` evaluate **both** operands.
`false & check()` still calls `check()`, and `true | check()` does too.
Use `&&` or `||` when the intended policy is conditional evaluation. Failure
and divergence in a required operand must remain observable in either compiler
mode. These distinctions follow the [Rust operator rules](https://doc.rust-lang.org/reference/expressions/operator-expr.html#arithmetic-and-logical-binary-operators).

The compiler regressions extract actual Cargo sources, compare native results,
exercise strict/lazy failures and recursive calls, and execute both optimization
modes. Separate fixed-width SMT models check bitwise identities and detect
mutations. Those model checks do not prove MIR extraction or CLVM lowering.

```sh
cargo test --locked -p chialisp-oxide-driver --test bitwise
cargo test --locked -p chialisp-oxide-ir --test bitwise
cargo test --locked -p chialisp-oxide-verifier --test bitwise
```

A caller can supply any `u8` at this boundary. The assertion rejects unknown
bits; no Rust type or mask supplies signature authorization, fee policy or
conservation. Checked integer shifts are supported; rotations and wrapping
addition remain outside the supported subset. SHA-256 is available only through the
[reviewed SDK bridge](/chialisp-oxide/reference/hashing/); it does not admit
arbitrary cryptographic dependencies or loops.

## Named value records

A local named struct can group policy terms while Rust checks its nominal type:

```rust
pub struct Terms {
    pub recipient: chialisp_oxide::PuzzleHash,
    pub amount: chialisp_oxide::Amount,
    pub deadline: u32,
}

#[chialisp_oxide::puzzle]
pub fn extend(mut terms: Terms, blocks: u32) -> Terms {
    terms.deadline = terms.deadline + blocks;
    terms
}
```

The CLVM value is a proper list in declaration order: `(recipient amount deadline)`.
This structural wire shape does not retain a Rust type name or native memory
layout. Nominal checks happen in rustc before lowering. Constructor fields may
appear in a different source order; their source evaluations remain strict.
Moves, by-value helpers, whole-value assignments and bounded field updates
operate on evaluated snapshots through the existing runtime function table.
Updates of ordinary tuple or array roots do not gain this record route.

The initial profile requires an ordinary local declaration with 1–256 named
fields, no generics, no custom `Drop` and no macro-generated declaration. Fields
may contain fixed-width integers, Boolean, unit, bounded fixed arrays, tuples,
reviewed transparent wrappers and nested admitted records. References, pointers,
platform-sized integers, owned values, arbitrary external structs, general enums,
`Cons` and `Option` payloads are excluded, including zero-length occurrences.
Explicit C, packed or aligned layouts are excluded. Existing SDK pair, optional,
tree and transparent-wrapper routes keep their separate contracts. An admitted
record can itself be a component of SDK `Cons` or an `Option`; this does
not admit those types as fields inside the record. `None` remains nil and
`Some(record)` is one list element containing the record proper list. Version 8
also admits that optional record as an entry value and by-value match target.

Original Rust field types and the native layout are checked before logical
erasure. Type discovery stays within 4,096 nodes and depth 32; field-update copies
are charged before construction against the existing expression/work limits.
The [actual Cargo regression tests](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-driver/tests/named_structs.rs)
compare native values and exact independently compiled Chialisp output in both
Rust optimization modes. These bounded regressions add no new IR format, nominal
wire identity or compiler-preservation proof.

## Fixed arrays and literal indexing

Use ordinary Rust arrays for a fixed number of values:

```rust
#[chialisp_oxide::puzzle]
pub fn repeated(value: u64) -> [u64; 3] {
    [value; 3]
}

#[chialisp_oxide::puzzle]
pub fn first_byte(bytes: [u8; 1]) -> u8 {
    bytes[0]
}
```

Non-byte arrays encode as proper lists in index order, including arrays of
supported local records. `[u8; N]` remains one exact-length atom, and a selected
byte becomes a canonical unsigned integer: byte contents `80` yield integer
128 with atom contents `0080`. Zero and leading zero bytes remain valid input
data. The existing immutable `&[u8; N]` representation supports the same literal
reads; this adds no general pointer/reference admission.

The frontend accepts in-bounds literal indices through an exact typed-MIR
bounds-check plan. It checks original fixed-value element types and every use
of the compiler's `usize` scratch locals. Dynamic indices, local-variable index
forwarding, indexed assignment, dynamic slices and element references remain
unsupported. The new indexing profile excludes `Option`, `Cons`, trees, drop
values and arbitrary containers; their existing construction capabilities are
separate. Fixed scalars, unit, tuples, nested arrays, supported transparent
wrappers and local records fit the read profile.

Computed initializers evaluate once in source order, including through a helper
that repeats its argument, and later assignments do not change an earlier array
snapshot. Discarded or zero-length arrays still evaluate failing/divergent
initializers; lazy branches remain lazy. Strict captures reuse existing runtime
IR while total argument/constant copy chains can retain the legacy route. The
[actual Cargo regressions](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-driver/tests/fixed_arrays.rs)
compare native Rust, independent Chialisp and both optimization modes, including
all 256 byte values and malformed shapes/lengths. They distinguish deliberate
VM resource controls from program rejection.

The 256-element type bound does not promise every layout fits existing backend
budgets. For example, the tests retain a source/IR-admitted 64-integer input
that exceeds the unchanged CLVM expansion nesting budget. That is a compiler
resource failure, not a policy rejection or proof of unsafe behavior. These
features add no new wire format or compiler-preservation proof.

## Named aggregate constants

Keep related policy data together using ordinary Rust constants:

```rust
const TERMS: (u8, u128, u16, bool) = (7, u128::MAX, 513, true);

#[chialisp_oxide::puzzle]
pub fn terms(_selector: u8) -> (u8, u128, u16, bool) {
    TERMS
}
```

The `_selector` still receives the default exact typed environment validation.
Tuples encode as proper CLVM lists; a validated `Cons<First, Rest>` constant
retains its explicit pair shape, including an improper tail. Nested tuples,
admitted local named records, bounded fixed arrays, transparent wrappers and
`const fn`-computed values are supported when all leaves are fixed-width integers, `bool`, unit or `[u8; N]` with `N <= 256`.
The complete original type graph must fit the existing depth/4,096-node limits,
and native storage must fit 65,536 bytes before evaluation. Rustc supplies actual
field offsets, so Rust padding and field reordering do not change logical order.

Reference/pointer, `Option`, tree and arbitrary container
payloads remain unsupported in this aggregate-constant route, including
zero-sized or zero-length occurrences. The existing borrowed-tree constant
route above is separate. Constants lower to existing IR constructors rather
than a new ABI or a new proof-model capability. The
[actual Cargo tests](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-driver/tests/aggregate_constants.rs)
check native values, independent Chialisp, both optimization modes, strict
failures, malformed environments and admission limits; these are finite tests,
not a compiler-preservation proof.

## Runtime calls and borrowed recursion

Ordinary Rust functions can recurse over the supported immutable tree type:

```rust
use chialisp_oxide::Tree;

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn rightmost<'a>(tree: &'a Tree<'a>) -> &'a Tree<'a> {
    match tree {
        Tree::Pair(_, rest) => rightmost(rest),
        Tree::Atom(_) => tree,
    }
}
```

Cargo extraction automatically selects version-4 IR for a reachable recursive
call graph. Each actual function body is lowered once into a finite closed
table; calls cannot select code supplied by the solution. Function-local typed
bindings retain unused calls and prevent reevaluating their results. Argument,
tuple, pair, and binary operand evaluation is strict and left to right. Each
callee starts with fresh locals; branch selection remains lazy.

Runtime functions support the named-record field updates described above. For
a tuple, array or transparent wrapper used on its own, construct and assign a
complete replacement instead. Constants and promoted shared
values retain their bounded acyclic lowering path. CFG loops and closure
captures remain unsupported. Dynamic CLVM application is available through the
[narrow SDK `apply` bridge](https://github.com/judeallred/chialisp-oxide/blob/main/docs/dynamic-apply.md)
over admitted borrowed trees; this does not add arbitrary Rust dependency or
function-pointer calls. Restricted owned tree construction is described below.
Runtime recursion does not bypass rustc borrow checking or permit escaping local
references. The input-policy attributes are unchanged; see the
[version-4 wire contract](/chialisp-oxide/reference/abi/#ir-version-4-and-export-markers).

Native comparisons use finite depths. Deliberate nontermination runs only inside
CLVM with a cost bound; resource exhaustion is inconclusive, not policy
rejection. The default scalar/tree SMT APIs reject runtime IR, including dead
calls and unused tables. The explicit
[acyclic scalar mode](/chialisp-oxide/verification/formal/#check-nonrecursive-scalar-helpers-explicitly) admits bounded,
nonrecursive scalar function tables over its stated typed post-ABI domain; it
still excludes recursion, SHA, apply, bytes, explicit pairs, optional outputs,
division/remainder and shifts.
Finite witness replay is execution evidence rather than a termination or
compiler-preservation proof. Durable regressions are documented
in [runtime calls](https://github.com/judeallred/chialisp-oxide/blob/main/docs/runtime-calls.md).

## Owned tree outputs with the alloc feature

Enable the facade's opt-in `alloc` Cargo feature to use `OwnedTree<'a>`:

```rust
pub enum OwnedTree<'a> {
    Atom(&'a [u8]),
    Pair(Box<Self>, Box<Self>),
}
```

Atom bytes remain borrowed; pair nodes have ordinary Rust ownership. Functions
can consume an input tree, move its children out of their boxes, and construct
an owned result without returning a reference to a stack-local node. Shared
reads followed by a move still pass normal rustc borrow checking. This enables
the [published prefix helper](/chialisp-oxide/tutorials/prefix-list/), which copies
a proper list's outer spine onto an arbitrary final tail.

This is a restricted value interpretation of the actual standard `Box` with
its default `Global` allocator, not general heap or standard-library support.
The compiler checks the recursive two-variant shape, actual Box identities,
constructor calls, and complete compiler-generated metadata use chains. It
keeps the original typed MIR intact. Atom/pair values have the existing tree
ABI; addresses, allocation identity, and Rust memory layout are not exposed.
Standard cleanup can be erased only for the validated pure owned shape, while
initializers and their failures remain strict.

Custom `Drop`, other Box pointees or allocator parameters, visible global
allocator overrides on owned exports, pointer comparisons, raw-pointer
conversion, leaks, and `mem::forget` remain rejected. The default facade has no
heap feature enabled; unannotated native Rust can still use ordinary containers.

Native owned-tree comparisons check whole input trees and the prefix result's
depth before constructing boxed inputs or entering the policy panic adapter.
Codec, allocation, and host-bound errors are inconclusive. Output encoding
consumes trees iteratively, including when it encounters an output resource
limit, so that an error does not leave a deep recursive output destructor.
These safeguards do not claim that arbitrary native Rust cannot exhaust memory.

The separate Lean experiment proves manually authored finite-tree algorithms
against list specifications. It does not prove Rust ownership erasure or
Rust/MIR/IR/CLVM preservation. Recursive owned exports still use version-4 IR,
which the current SMT model APIs reject.

## Explicit wire pairs

`chialisp_oxide::Cons<Head, Tail>` is the narrow vetted SDK struct exception.
Struct literals, field reads, nested concrete supported types, moves and
snapshot-preserving complete reassignment are checked through actual MIR.
Its two fields encode one CLVM pair, while tuples retain their proper-list ABI.
A `Tree` tail may be an atom or an improper pair. Field construction and unused
initializers stay strict; the default input ABI validates both fields.

This does not admit structs outside the separately checked local-record and
transparent-wrapper profiles, generic helper functions or custom Drop.
The frontend validates the real SDK ADT identity and shape rather than its
spelling. Pair-capable programs use IR v5, v6 with optional outputs, v7 with
integer casts, or v8 with optional entry/inspection capability. They retain proper argument frames; optional-capable programs
conservatively skip Rust IR refinement and inlining. Current SMT models reject
both capabilities explicitly. See the
[wire-pair ABI](/chialisp-oxide/reference/abi/#explicit-pairs-with-conshead-tail).

## Rejected features

The narrow borrowed-tree exception supports the SDK's immutable `Tree<'a>`
representation: `Atom(&'a [u8])` and `Pair(&'a Tree<'a>, &'a Tree<'a>)`, shared
reborrows, construction, pattern matching, lifetime-only helpers and supported
promoted constants. The frontend validates the actual enum shape and lowers
real helper/constant MIR; a familiar name does not grant intrinsic semantics.
It does not admit arbitrary enums, mutable references, or allocation. Recursive
functions can traverse these borrowed values without changing Rust ownership. See the [raw environment ABI](/chialisp-oxide/reference/abi/#explicit-raw-tree-environments).

General references, type/const generic functions, arbitrary external function calls, CFG loops,
allocation beyond the restricted owned-tree Box route, structs outside the
closed local-record/transparent-wrapper profiles and general enums,
floating point, and casts outside the fixed-width integer profile are not
implemented in the accepted subset. Boolean, character, floating-point and
pointer casts are excluded. User `usize` and `isize` values and casts remain
unsupported; the closed compiler-generated metadata exceptions do not expose
those target-sized types as an authoring API. Unsafe code is forbidden by rustc's
`-Funsafe-code`, including unsafe code that MIR might otherwise eliminate.

The transparent-wrapper exception requires a nongeneric declaration, local or
from the vetted SDK, with one supported nonzero-size field, no custom `Drop`, and no
macro-generated declaration. Extra fields, even zero-size ones, are rejected.
Other structs and enums remain unsupported outside the separate local
[named-record profile](#named-value-records); unrelated external wrappers remain
unsupported. Concrete vetted SDK helper calls are supported;
resolving an ordinary Rust dependency through Cargo does not establish that its
function bodies can be lowered.

Copying an amount value is allowed. Neither these traits nor the domain names
enforce asset conservation, authorization, or double-spend prevention.

These restrictions apply to exported reachable code. The Cargo consumer's
unannotated native-only formatting/allocation function can compile and run
normally because it is not reachable from a marked entry. The selected library
and SDK still forbid unsafe code and unstable source features.

The internal IR enumerates some future operations. An enum variant is not a
supported language feature: validation rejects operations without implemented,
tested lowering.

## Arithmetic and evaluation

Overflow checks and debug assertions are enabled independently of the host
Cargo profile. `u64::MAX + 1` must not become an unbounded CLVM integer.
Explicit wrapping and overflowing arithmetic methods are rejected.

The [Rust operator reference](https://doc.rust-lang.org/reference/expressions/operator-expr.html)
defines the source arithmetic contract.

Checked integer `/` and `%` retain Rust truncation toward zero, including
rejection for a zero divisor and for signed `MIN / -1` or `MIN % -1`. They are
not CLVM arbitrary-width floor division. Integer shifts reject negative or
width-sized counts, discard shifted-out left bits, and use arithmetic right
shift for signed values. Explicit raw CLVM helpers use tree/atom
inputs and the reviewed `apply` bridge; their native test route requires the
`vm` feature and executes the pinned VM. These interfaces have different numeric
and byte contracts and do not add those operations to the scalar proof models.

Unused failing computations are still significant. If a helper rejects and its
returned value is unused, strict Rust evaluation requires rejection. The IR's
sequence nodes retain this obligation for the legacy acyclic route. Runtime
IR uses strict typed bindings and explicitly evaluates operands once, left to
right. A failure or diverging call in an unchosen branch remains lazy.

## Integer casts preserve Rust conversion rules

An `as` conversion changes the numeric value according to Rust's
[integer-cast rules](https://doc.rust-lang.org/reference/expressions/operator-expr.html#numeric-cast).
It is not a checked-arithmetic operation or an application range check. Both
source and destination must be one of `i8`, `i16`, `i32`, `i64`, `i128`, `u8`,
`u16`, `u32`, `u64` or `u128`.

```rust
#[chialisp_oxide::puzzle]
pub fn convert(value: i16) -> (i8, u8, i32, u32) {
    (value as i8, value as u8, value as i32, value as u32)
}
```

For input `-1`, the result is `(-1, 255, -1, 4294967295)`. Signed widening
preserves the sign; widening a negative signed source to an unsigned destination
also extends the sign before interpreting the destination's bits. For input
`256`, the result is `(0, 0, 256, 256)`: both narrow conversions retain only the
low eight bits. Converting `128` to `i8` produces `-128`, while converting it to
`u8` produces `128`. No overflow panic is implied by any of these casts.

By contrast, checked `u8` addition of `255 + 1` rejects. If a conversion must
reject out-of-range input, write that policy before the cast:

```rust
#[chialisp_oxide::puzzle]
pub fn checked_byte(value: i16) -> u8 {
    assert!(value >= 0 && value <= 255);
    value as u8
}
```

This returns `255` for `255` and rejects `256` or `-1`. A cast alone would not
establish that constraint. Source operands still evaluate once, including a
failing or diverging operand whose converted result is discarded; lazy branches
retain Rust's ordinary behavior.

An integer conversion does not reinterpret the serialized CLVM atom. The source
ABI still rejects noncanonical or out-of-range input. The `u8` integer `128`
encodes as canonical integer bytes `0x0080`; a one-byte array containing `128`
encodes as `0x80`. A numeric cast does not turn one representation into the other.
The explicit raw CLVM interfaces retain their own atom contracts.

Programs retaining an integer-cast node in the checked IR use the version-7
capability. A source cast that rustc eliminates, such as an identity cast,
does not force that capability. Older readers reject the version-7 envelope
rather than silently treating a retained cast as ordinary arithmetic.
Cast-capable programs conservatively skip optional Rust IR
refinement and helper inlining; backend optimization may still run. Programs
without casts retain their existing lowering route. No cost improvement follows
from adding the capability.

Current proof models reject cast-capable programs, including cast nodes retained
in discarded expressions or unused IR functions. The conversion examples and compiler regressions are
finite execution evidence, not compiler-preservation proofs.

This profile does not add Boolean/character/float/pointer casts, target-sized
`usize`/`isize`, `TryFrom`, wrapping methods, or general trait calls. The native
valid [negative consumer](/chialisp-oxide/tutorials/typed-sdk/#observe-a-typed-api-failure)
uses a Boolean-to-`u8` cast to demonstrate that ordinary Rust checking and the
additional subset check remain distinct.

## Bounded compilation

The frontend limits legacy helper inlining to 32 calls, active source
control-flow lowering to 64 blocks, and total visited blocks to 10,000. Expressions are limited to
8,192 nodes and depth 64, with 500,000 cumulative expression nodes of lowering
work. Types are limited to 4,096 nodes and depth 32; functions have at most
64 parameters and 2,048 MIR locals. Aggregate construction and cloning are
bounded before replication.

The IR independently validates the complete entry and every function body:
64 functions, 64 parameters per function, 20,000 combined expression nodes,
expression depth 256, 20,000 declared type nodes, type depth 32, and 500,000
validation-work units. Runtime lowering checks an additional 128 new frame
bindings beyond arguments and 500,000 work units, including local-type copies,
before cloning scopes. Optimized runtime lowering separately limits live operand
descriptors to 128 and direct expression descent to 192; forwarding values must
not bypass the compiler's resource limits. A later estimated four-million-unit/depth-96 expansion
check can reject an otherwise structurally valid table. These bounds constrain
compiler work; they do not establish affordable or terminating execution for
every input.

## Host-checked source, not a CLVM rustc target

Source expansion currently uses the pinned host compiler configuration. That
includes `cfg(target_*)`, `cfg(debug_assertions)`, pointer-size expressions, and
host standard-library availability. Avoid target-dependent source in
reproducible puzzles. Ordinary `std` or `no_std` crates are not automatically
compatible with the on-chain subset.

Compile-fail fixtures exercise unsupported features, Rust type/borrow failures,
and unsafe code. They are useful regression evidence; they are not an argument
that a general Rust-to-CLVM translation has been proved correct.

The [delegated-conditions lesson](/chialisp-oxide/tutorials/delegated-conditions/)
uses `Cons<(u64, &Tree, [u8; 32]), &Tree>`: a proper-list signature condition
followed by the original arbitrary conditions tail. The owned digest and
borrowed tree fields keep their ordinary Rust lifetimes. This does not grant
arbitrary generic structs, on-chain BLS operations or proof-model support.
