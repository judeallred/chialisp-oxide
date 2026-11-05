# Internal local function-pointer values

**WIP prototype. Not ready for real funds or production use.**

The version-9 capability admits immutable function-pointer values whose targets
are ordinary safe local Rust function items or checked noncapturing closures
coerced to an ordinary function-pointer type. It lets a checked local helper
receive a callback and reuse the same body with different local functions:

```rust
use chialisp_oxide::puzzle;

type Step = fn(u64) -> u64;
fn increment(value: u64) -> u64 {
    assert!(value < 100);
    value + 1
}
fn double(value: u64) -> u64 {
    assert!(value < 100);
    value * 2
}
fn invoke(step: Step, value: u64) -> u64 {
    step(value)
}
#[puzzle]
pub fn choose(flag: bool, value: u64) -> u64 {
    let step: Step = if flag { increment } else { double };
    invoke(step, value)
}
```

A noncapturing closure can use the same helper through Rust's normal coercion:

```rust
#[puzzle]
pub fn through_closure(value: u64) -> u64 {
    let step: Step = |n| n + 1;
    invoke(step, value)
}
```

Its body refers only to its parameter. A closure that reads a surrounding local
has a capture, even when that value occupies no bytes. The separate closed move
capture route below admits immutable creation snapshots and concrete local calls.
Noncapturing direct closure calls still require the explicit pointer coercion;
returning/storing closure values and generic `F: Fn` helpers remain unsupported.

## Immutable move captures

A concrete local `move` closure may capture whole local values with supported
Copy, drop-free primitive, fixed tuple/array, or actual immutable SDK `Tree` types:

```rust
#[puzzle]
pub fn snapshot(mut base: u64) -> (u64, u64, u64) {
    let step = move |n: u64| base + n;
    base += 100;
    (step(1), step(2), base)
}
```

An input of 7 returns `(8, 9, 107)`: the closure retains its creation snapshot.
The outer scalar may change later. The closure binding must be immutable and
have one MIR definition; immutable copies, shadowing, repeated calls and separate
callee frames retain paired code/environment identities. Captured initializers
still run once when an unused closure is constructed; an unused body stays lazy.
Overflow keeps ordinary Rust checked behavior.

This route requires an actual plain `move` closure, whole-root ByValue captures,
validated original concrete types, the actual core `Fn` call identity and exact
local closure resolution. A moved `Cell` can be Fn and drop-free while mutating
between calls, so those properties alone never authorize a capture. Shared,
projected, mutable/interior-mutable, custom-ADT, callable and owned/drop captures,
FnMut/FnOnce, nested captures, explicit closure references, container/return
escapes, same-type conditional joins and generic/trait-object helpers remain
closed. Captured closures cannot be passed as anonymous generic callbacks; this
slice does not complete the modern-Chialisp ABC example.

The frontend pairs an immutable compiler-owned function reference with an
explicit captured-environment tuple. Each concrete Fn call uses existing v9
indirect dispatch with the environment as the helper's first argument. The IR
stores no Callable inside an aggregate and introduces no new wire node/version.
Original MIR receiver/upvar types and exact definition/use checks remain visible
through normalization, including captured array field indexing. Source capture
metadata is not a public ABI or nominal authentication mechanism.

The `choose` export still accepts exactly a canonical boolean and unsigned 64-bit
integer. `(1 7)` returns 8; `(() 7)` returns 14. An amount of 100 rejects through
the selected helper's assertion. This is an arithmetic example, not coin
conditions or a spend. The caller does not supply a callable, function-table
index or program bytes.

## Closed source contract

The driver checks the real rustc `ReifyFnPointer` coercion, the local function's
actual `FnDef`, its original signature and its complete MIR body. The separate
`ClosureFnPointer(Safe)` route checks the actual local `DefKind::Closure`, empty
capture metadata and constructor operands, immutable `Fn` kind, original safe
signatures, and the complete body. It validates Rust's outer argument tuple
against the flat body parameters and excludes the exact unused compiler receiver;
it does not admit general `extern "rust-call"` functions or trait calls. ZeroSized
closure operands are read as already evaluated type metadata without CTFE or
pointer bytes. Names do not
grant callable or primitive behavior. Every reified target is admitted, including
one that a particular input does not select. Existing overflow/compiler flags,
SDK identity checks and unsupported-body diagnostics still apply.

Promoted literals inside an admitted closure retain rustc's three compiler-owned
kind/signature/upvar parameters. Their source identity arguments are checked
against the exact owner, then instantiated with its fully validated metadata
through rustc's API. Nested promoted references require the same active context.
The original MIR, original Rust types and acyclic constant limits stay visible;
body/type work and possible copies are bounded before cloning. This does not
admit source type/const generics or static function pointers. This promoted-literal
contract applies to the zero-environment coercion route; captured bodies do not
acquire broader promoted-constant support.

Only safe ordinary Rust signatures are accepted, with bounded lifetime-only
binders over the already supported immutable tree/byte shapes. Type/const
generic targets, foreign/unsafe/variadic functions and broader borrowed pointee
admission remain excluded. Callable bindings may be copied and shadowed; source
mutable callback bindings, pointer comparisons, address/integer conversions,
constant/static pointer values and general `Fn`/`FnOnce` trait APIs remain
outside this profile. The concrete move-capture call is the closed exception
described above.

When retained callable MIR activates this profile, every discovered local owner
uses a conservative typechecked-HIR rule: any mutable function-pointer binding in that owner rejects. When a retained
zero-environment closure coercion or retained move capture activates the closure route, mutable intermediate
closure bindings also reject, including unused, unreachable, shadowed and macro-generated
bindings. This checks actual binding identities and types, without matching
debug names or source spans to optimized MIR locals. The owner walk is bounded
to 10,000 nodes and depth 64, preflighted before querying cached typecheck metadata
and charged against the existing compiler work budget.

Callables are internal local values or direct helper parameters. They cannot
appear in exported argument/result types, function returns, tuples, named
records, arrays, `Option`, explicit pairs, wrappers or owned payloads. A complete
raw `Tree` environment does not relax this boundary.

## Evaluation and representation

A function reference identifies a compiler-owned entry in the validated local
function table. Indirect dispatch is bounded by the same source/IR work and
function-table limits as ordinary runtime calls. It never executes an
input-selected CLVM program through this mechanism. The SDK's explicit `apply`
operation remains a separate primitive with its own reviewed semantics.

The callable operand and each argument are captured once in source order before
dispatch. The selected function has fresh parameter/local slots. An unused
argument that can fail still evaluates. Copying or shadowing a callback does
not change an earlier captured value. Resource exhaustion remains inconclusive,
rather than evidence that a source policy rejected.

Retained callable types/nodes use IR version 9. Earlier readers fail closed;
callable nodes cannot be inserted into older envelopes. Programs without
retained callable IR keep their earlier version/lowering route. Rustc may
eliminate a source-level function item or cast before extraction; source spelling
alone does not force v9. All current proof/model APIs reject callable capability,
including dead or unused occurrences. A closure body retains the existing
64-argument MIR bound including its hidden receiver, so this initial route
admits at most 63 explicit closure parameters. Optional Rust IR refinement/inlining stays
conservatively disabled for callable-bearing programs; existing backend
optimization is separate from a callable-rewrite or preservation proof.

## Reproduce the finite tests

After a coordinated host build, the focused source tests are:

```sh
cargo test -p chialisp-oxide-driver --test local_callables --test noncapturing_closures --test captured_closures --locked
cargo test -p chialisp-oxide-ir --test callables --locked
cargo test -p chialisp-oxide-verifier --test callables --locked
```

The Cargo fixture uses the actual local Rust helpers, native unit tests and
fresh extraction. Both emitted Rust modes compare exact outputs with independent
Chialisp and Rue source. The raw callback map is separately compared with the
unchanged pinned named/lambda modern-Chialisp sources using the recorded cl24
provider. Those map observations admit local Rust function values. Separate move-capture
fixtures check native creation snapshots, fixed values, fresh frames and malformed
ABI/typed failures against independent sources and a previous-v9 control. Neither
lane completes generic trait support, a whole tutorial page or a coin spend.

Constructed IR tests check wire/type/target corruption, strict failure order,
resource limits and model rejection separately. Finite old-artifact comparisons
are regression controls, not a compiler-preservation proof or authenticated
record of execution.
