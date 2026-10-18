# Runtime calls in checked Rust

Status: **implemented for the supported Rust subset**. Cargo extraction supports
self recursion, mutual recursion, and non-tail calls through a finite,
compiler-owned function table. Ordinary rustc type and borrow checking runs
before extraction. This is runtime execution support, not a termination theorem
or a general compiler-preservation proof. CFG loops, closures, general allocation,
and solution-selected application remain unsupported. The optional owned-tree
subset described below supports logical pair construction.

## Ordinary borrowed Rust

A raw entry can traverse an immutable borrowed tree without an allocating return
type or a special recursion annotation:

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

This follows each right child and preserves every byte of the final atom. It
accepts proper and improper trees under the explicit raw-environment contract.
The driver discovers actual reachable function identities and signatures. If
that graph contains a cycle, it lowers each function once and makes the entry
call function zero. Acyclic source graphs retain the existing expression-inlining
route and legacy wire formats. SDK helper bodies remain real inspected MIR;
their names do not select opaque implementations. Supported borrowed/promoted
constants still use the separate, bounded acyclic constant-lowering path.

The frontend preserves the existing restrictions on SDK dependencies,
lifetime-only generics, immutable references, unsafe code, destructors, and MIR
operations. Runtime functions additionally reject projected assignments: assign
a complete supported value rather than mutating a tuple field. Rust still
rejects escaping references to stack-local trees. Reverse and append that retain
new accumulation nodes need a genuine ownership/arena design; runtime calls do
not make those references valid. The opt-in `OwnedTree` representation now
supports a restricted owned accumulator without escaping references; see
[owned trees](owned-trees.md). General collections and dynamic atom storage
remain unsupported.

## Version-4 owned IR and lexical evaluation

A program with a function table or any `Call`, `Let`, or `Local` node uses:

```text
{ ir_version: 4, environment, program: {
    name, parameters, result, body, functions
} }

Function { id, name, parameters, result, body }
Call     { function_id, arguments }
Let      { local_id, ty, value, body }
Local    { local_id }
```

IDs are dense function-table indices. Function names are diagnostic labels.
Each function has its own `Arg` scope and starts without caller locals. `Let`
checks its initializer outside the new binding, evaluates it once before its
body, and retains evaluation even when the result is unused. Its explicit type
also permits checking the body of a rejecting initializer. Active local IDs
cannot duplicate one another; sibling lexical scopes may reuse them.

The source frontend binds non-atomic MIR definitions and call results to fresh
locals. Copying a previously bound value does not rerun its call. The backend
also accepts expression operands in handwritten valid IR: call arguments,
tuples, tree-pair children, and binary operands evaluate once, left to right.
Continuations implement these rules explicitly, instead of relying on CLVM's
operand scheduling or the legacy `Rest(Cons(first, then))` sequence encoding.
Unchosen `If` branches remain lazy. Rust's
[operand evaluation order](https://doc.rust-lang.org/reference/expressions.html#evaluation-order-of-operands)
therefore remains relevant when an operand rejects or diverges.

Every runtime frame is `(compiler_function_table . slots)`. Initial slots are
the arguments; a retained binding prepends one slot and enters a quoted
continuation. Binding identities rebase argument/local paths after extension.
A call starts a fresh slot list containing its arguments, while forwarding the
closed table. Quoted bodies refer to table paths, not cyclic compiler-arena
references. The bootstrap constructs that table; an attacker-controlled
environment cannot supply a callee or executable body through this convention.

Version 4 is independent of input policy. Existing strict, ignored-nullary, and
raw-tree ABI checks still apply, and the complete strict argument list is
validated before bootstrap. Export attributes and metadata markers remain the
existing v1/v2/v3 input-policy markers; there is no new macro attribute for
runtime calls. Runtime-free programs retain their v1/v2/v3 serialization.
Legacy readers explicitly reject new runtime nodes and function metadata.
A v4 envelope is accepted only if it contains a runtime node or a nonempty
function table. A legacy-only body with `functions: []` is rejected, rather than
silently downgraded to the legacy evaluator or admitted to a model as legacy IR.

## Optimization and resource boundaries

The optimized pipeline is checked constant folding → forward success facts →
constant folding again → legacy tree guards → bounded helper inlining → runtime
frame elision → Rue LIR optimization → static pair code generation. Off retains the original lowering
and skips these optional stages. The extracted source IR and its wire version
stay unchanged; every owned IR rewrite is validated before lowering.

Constant folding respects Rust's integer width and checked overflow. An unused
total constant or match discriminant can disappear, but a call, SHA computation,
checked projection or potentially overflowing arithmetic still executes. Known
aggregate values share analysis metadata instead of expanding copied trees.

Forward facts identify immutable arguments, lexical bindings and projections.
After `First(value)` returns, `value` is known to be a pair on that continuation.
A subsequent `IsPair(value)` can become true. Conditional branches carry their
own facts; a merge keeps facts shared by every branch that can return. A branch
that explicitly rejects contributes no successful continuation. Calls have no
inferred termination or return summaries, and each function starts a fresh
analysis scope. Rustc still checks ordinary source types; these refinements
belong to the owned optimizer over validated typed IR.

Bounded helper inlining removes small, closed, nonrecursive calls and direct
identity-argument entry wrappers. Arguments become strict left-to-right lexical
bindings even when the callee does not use them. Fresh local identities prevent
capture; recursive SCCs remain calls. All function bodies are validated before
unreachable entries are removed and remaining call IDs are densely remapped.
Work, copied text, type/node counts, depth and runtime frame growth are bounded
before expansion. Removing the final runtime construct is skipped because the
current public IR wire format has no independent evaluation-order field.

The public optimizer also checks the complete refinement result at this wire
boundary. A possibly failing result retains the original runtime program if
optimization would otherwise remove its last mode marker. A closed, iterative
total-result whitelist permits constants, decoded arguments and total typed
constructors/comparisons to keep their reductions. It checks every child,
including discarded tuple fields; checked raw-tree projections/casts, checked
arithmetic, calls and hashing are excluded. Validated tuple/byte field lookup is
total when its entire operand is total. Private compilation carries its original
mode separately.

Compilation validates before scanning for runtime nodes. If inlining exceeds an
existing lowering or expansion budget, a private typed limit triggers one retry
of the preceding optimized IR. Other errors propagate. This fallback preserves
admission through the previous lowering route without changing optional-pass
limits or treating resource exhaustion as policy rejection.

In runtime operand lists, `Arg`, `Local`, and literals can pass directly to the
consumer without a temporary frame. Argument/local descriptors retain their
binding identities until the final consumer scope is known, so paths account for
any intervening frames. Literal descriptors contain values independent of the
frame. The compiler consumes these leaves iteratively. Non-leaf operands retain
their ordered, once-only continuation bindings, including operands a callee or
tuple projection later ignores.

Single-consumer operations also avoid a redundant frame: `Not`, `TreeIsPair`,
`TreeFirst`, `TreeRest`, `AtomIsEmpty`, and tuple/byte `Field` lower their operand
once directly. A checked `TreeAtomValue` uses its operand in both the atom guard
and returned value, so a non-leaf operand still gets a once-only binding. The
exact `TreeAtomValue(TreeAtom(payload))` shape needs no shape guard: validated
construction already establishes atom shape. Its payload still evaluates once,
including calls and SHA. The typed cast remains in IR. Other non-leaf casts keep
their bindings to prevent an operand from running twice.

One exact `Let` shape also needs no continuation: when its body is precisely
`Local` of that binding, lowering returns the initializer directly. The
initializer still evaluates once in the outer lexical scope, preserving its
rejection, divergence, or checked overflow. Total leaf aliases also need no
physical frame: their resolved identities are rebased in the final consumer
scope. Other `Let` bindings keep strict initializers even when unused; a
`Sequence` may drop only an established total first expression. These reductions
do not infer general purity or termination.

The older backward pair-guard pass leaves every runtime program unchanged. Its
successful-evaluation facts alone cannot justify removing this earlier guard:

```text
If(IsPair(arg), Sequence(spin(), First(arg)), Reject)
```

For an atom, the guard rejects before entering `spin`. Dropping it would turn
that rejection into divergence. Giving a call zero pair facts is insufficient
because a later projection can still contribute through a union rule. Frame
reduction does not enable this guard rewrite. Finite, rejection, and deliberate
resource controls exercise both compilation modes.

Validation bounds the whole program: 64 functions, 64 parameters per function,
20,000 expression nodes across the entry and every table body, expression depth
256, 20,000 declared type nodes, type depth 32, and 500,000 validation-work units.
It checks unused functions, unreachable branches, arguments, and lexical bodies.
Source extraction has additional MIR/expression limits. Runtime lowering checks
before copying scopes: at most 128 additional lexical/temporary slots beyond
arguments, and 500,000 cumulative work units including copied local types.
Optimized lowering separately limits pending operand descriptors to 128 and
active expression-lowering depth to 192. Forwarding a value or lowering a unary
operation directly therefore cannot evade compilation bounds by avoiding a
physical slot. The limits are checked before growing the corresponding vectors
or descending further; they are compiler resource bounds, not execution-depth
or termination guarantees.
The new recursive analyses first scan every body iteratively. An expression
deeper than 64 skips the new analyses and follows the existing compiler route;
this does not reject otherwise valid IR. Analysis work and literal copies also
have explicit budgets. Validation still checks dead branches and unused
functions before optional optimization.

The later code-generation expansion check remains four million estimated units
and depth 96. A valid 64-function table is not guaranteed to fit those budgets.

VM execution always has a finite cost budget. Under the shared typed failure
protocol, cost, stack, timeout, and allocation limits are resource failures;
they are inconclusive rather than program rejection. Infrastructure failures
are also inconclusive. A program failure does not by itself identify the
intended policy assertion. Native Rust has a different call stack and resource
boundary: finite native tests use safe depths, and deliberate nontermination
runs only inside the cost-bound VM.

## Durable evidence and remaining work

The compact [`runtime-frame-baseline.json`](../corpus/runtime-frame-baseline.json)
pins commit `6f1e197ef6647683f4c3d7dd171c5e7e4317b01c`, its full report's
SHA-256/Git blob, and both Rust builds for the two recursive corpus helpers.
[`check-runtime-frames.py`](../scripts/check-runtime-frames.py) retains exact
source, contract, outcome, reference, ledger and unoptimized evidence across all
15 fixtures while permitting only those two optimized Rust artifacts and metrics
to change. Its corruption controls check that boundary. Optional `--replay`
runs all 58 retained old-byte executions through the pinned current VM, including
exact costs and typed failures; `--verify-git` additionally reconstructs the
compact snapshot from the historical Git report and Rust sources.

Measured optimized sizes fell from 339 to 175 bytes for `rightmost-tree` and
743 to 369 bytes for `last-list-element`. On `(1 2 3)`, the respective optimized
costs fell from 17,423 to 7,594 and 25,933 to 11,568. All 21 accepted named cases
have lower recorded costs. This is finite measurement, not universal cost
dominance: the current Chialisp/Rue implementations remain substantially cheaper.
Changing the preserved evidence in future work requires an explicit reviewed
comparison scope rather than weakening this baseline silently.

- [`driver/tests/runtime_calls.rs`](../crates/chialisp-oxide-driver/tests/runtime_calls.rs)
  extracts actual Cargo/standalone Rust, compares finite native and optimized/
  unoptimized VM outcomes, and covers self/mutual/non-tail calls, unused failing
  results, lazy branches, checked overflow, SDK constants, and rejected features.
- [`ir/tests/runtime_evaluation.rs`](../crates/chialisp-oxide-ir/tests/runtime_evaluation.rs)
  checks function selection, lexical paths, byte-field helper contexts, all
  external ABIs, strict argument order, lazy branches, divergence-sensitive
  guards, and a measured one-call versus two-call budget control.
- [`ir/tests/runtime_optimization.rs`](../crates/chialisp-oxide-ir/tests/runtime_optimization.rs)
  adds eight executed regressions comparing the original off path with optimized
  lowering: heterogeneous forwarding, fresh nested tuple/byte scopes, discarded
  strict operands, unary projection siblings, once-only checked atom extraction,
  tail-identity initializers, divergence-sensitive selectors, and unused nested
  ABI arguments. Exact expected outputs and typed failures remain the oracle;
  a deliberate duplicate call supplies the once-only cost control.
- [`ir/tests/runtime_validation.rs`](../crates/chialisp-oxide-ir/tests/runtime_validation.rs)
  and [`ir/tests/wire.rs`](../crates/chialisp-oxide-ir/tests/wire.rs) check lexical,
  signature, global-budget, and versioning boundaries, including runtime nodes
  injected into old envelopes. Resource cases also cover nested wide operands,
  effectful operands that still require frames, and optimized direct-unary depth.
- [`verifier/tests/runtime_boundaries.rs`](../crates/chialisp-oxide-verifier/tests/runtime_boundaries.rs)
  requires every current scalar/tree model API to reject runtime IR, including
  dead calls and unused tables. Finite witness replay is allowed as execution
  evidence; it does not make that IR a supported proof input.

These are executed regressions over their specified inputs, not universal
termination or source-to-CLVM preservation proofs. A separate acyclic call model,
then explicit recursive termination/induction obligations, remain future work.
The first published helper adaptation is Chia gaming's
[`last_inner` helper](https://github.com/Chia-Network/chia-gaming/blob/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/clsp/std/last.clinc#L8-L13).
The `last-list-element` fixture embeds that exact helper span in a visible
Chialisp module adapter and independently compiles it, checked Rust, and Rue.
Rust `Tree::Atom([])` uses a narrow typed-MIR recognizer; pointer and `usize`
metadata cannot escape the validated pattern chain. Nonempty `0x00` is not nil;
improper nonempty atom tails reject. The returned final element is an arbitrary
unchanged tree. The source/license/commit/tree/span/wrapper gate has mutation
controls in `scripts/check-canonical-helpers.py`. This is one helper, not an
entire source library or canonical module-identity claim. Its finite comparisons
remain separate from recursive equivalence or preservation proofs. See
[verification scope](verification.md) and the
[walkthrough](https://judeallred.github.io/chialisp-oxide/tutorials/last-list-element/).

## Historical backend experiment

[`experiments/backend/tests/runtime_calls.rs`](../experiments/backend/tests/runtime_calls.rs)
predates Rust extraction. It constructs Rue LIR directly with the simpler
`(table . argument_list)` frame and checks 24 finite traversals plus two explicit
cost-exhaustion controls. Its original 2026-10-16 probe measured self-recursive
artifacts of 117/109 bytes and mutual-recursive artifacts of 169/157 bytes
(optimization off/on). At depth 2,048 the costs were respectively
1,436,397/1,381,101 and 1,440,564/1,385,268. These are historical LIR-probe
measurements, not current frontend costs, canonical hashes, or guarantees.

The pinned Rue
[function lowering](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-hir/src/lower.rs#L143-L176),
[call lowering](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-hir/src/lower.rs#L429-L454),
and [Run/Closure code generation](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-lir/src/codegen.rs#L48-L92)
provided implementation references. The implemented closed-call route uses
`Run`; it does not expose first-class closures or arbitrary application.
