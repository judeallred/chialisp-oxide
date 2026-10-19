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
- Local values, tuple fields, comparisons, boolean negation, `if`, integer and
  boolean `match`, and acyclic control flow.
- Checked `+`, `-`, and `*`; overflow rejects the puzzle.
- Built-in integer `&`, `|`, `^` and `!` at each supported signed/unsigned
  width; `bool` also supports strict `&`, `|` and `^`. These do not add
  wrapping arithmetic or shift support. See [bit masks](#bit-masks-and-evaluation-order).
- Plain `assert!` as puzzle rejection. Formatted assertions and panic messages
  are rejected because their formatting calls are outside the current subset.
- Local/SDK helpers, including lifetime-only generics for borrowed and owned trees. Acyclic
  source graphs inline into expressions; self, mutual, and non-tail recursion
  use a closed runtime function table. Type/const generic functions remain rejected.
- Named scalar and fixed-byte constants, including supported transparent wrappers
  of those values: typed policy amounts, recipient hashes, and public fixture keys.
  The restricted tree route also lowers supported promoted shared constants from their real MIR. General pointer and tuple constants remain unsupported.
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
- Vetted by-value SDK methods such as `Amount::from_mojos`, `.mojos()`,
  `.is_at_most()`, `Timestamp::from_unix_seconds`, and `.unix_seconds()`.
  The SDK's small transparent values support implicit `Copy` reuse. Explicit
  `.clone()` calls remain unsupported because their borrowed receiver and trait
  dispatch are outside the accepted subset.
- Supported MIR constant array projections. Ordinary indexing that needs an
  index local or reference is rejected.
- Empty-slice patterns on supported atom slices, such as `Tree::Atom([])` or
  `OwnedTree::Atom([])`.
  Nil matches; nonempty zero bytes do not. The frontend validates the complete
  pinned MIR metadata-test chain and erases its compiler-generated pointer and
  `usize` intermediates. General length methods, indexing and raw pointers
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
conservation. SHA256, integer shifts, rotations and wrapping addition remain
outside the supported subset.

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

Runtime functions currently reject assignments through tuple/field projections;
construct and assign a complete value instead. Constants and promoted shared
values retain their bounded acyclic lowering path. CFG loops, closure captures,
and general `apply` remain unsupported. Restricted owned tree construction is
described below. Runtime
recursion does not bypass rustc borrow checking or permit escaping local
references. The input-policy attributes are unchanged; see the
[version-4 wire contract](/chialisp-oxide/reference/abi/#ir-version-4-and-export-markers).

Native comparisons use finite depths. Deliberate nontermination runs only inside
CLVM with a cost bound; resource exhaustion is inconclusive, not policy
rejection. Current scalar/tree SMT models reject all runtime IR, including dead
calls and unused tables. Finite witness replay is execution evidence rather than
a termination or compiler-preservation proof. Durable regressions are documented
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

This does not admit general structs, generic helper functions or custom Drop.
The frontend validates the real SDK ADT identity and shape rather than its
spelling. Pair-capable programs use IR v5 and conservatively keep the preceding
facts/frame lowering; the new known-atom/unary optimizations are not extended
to them. Current SMT models reject this capability explicitly. See the
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
allocation beyond the restricted owned-tree Box route, general structs/enums, casts, floating point, shifts,
division, and remainder are not implemented in the accepted subset. `usize`
and `isize` are rejected as ABI types. Unsafe code is forbidden by rustc's
`-Funsafe-code`, including unsafe code that MIR might otherwise eliminate.

The transparent-wrapper exception requires a nongeneric declaration, local or
from the vetted SDK, with one supported nonzero-size field, no custom `Drop`, and no
macro-generated declaration. Extra fields, even zero-size ones, are rejected.
This support does not make ordinary structs, enums, or unrelated external wrapper
types available on-chain. Concrete vetted SDK helper calls are supported;
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

Unused failing computations are still significant. If a helper rejects and its
returned value is unused, strict Rust evaluation requires rejection. The IR's
sequence nodes retain this obligation for the legacy acyclic route. Runtime
IR uses strict typed bindings and explicitly evaluates operands once, left to
right. A failure or diverging call in an unchosen branch remains lazy.

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
