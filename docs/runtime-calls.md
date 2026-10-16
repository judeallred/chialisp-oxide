# Runtime calls: backend experiment and proposed Rust integration

Status: **backend-only experiment**. The Rust frontend still rejects recursive
calls and loops, and the owned IR has no function table or call/binding nodes.
The experiment constructs Rue LIR directly; it does not extract recursive Rust
MIR or establish compiler preservation. No SDK symbol receives an opaque
intrinsic meaning.

The durable reproduction is
[`experiments/backend/tests/runtime_calls.rs`](../experiments/backend/tests/runtime_calls.rs).
It uses the project's pinned Rue LIR, including its documented local fixes,
clvmr 0.21.0, `ChiaDialect::new(MEMPOOL_MODE)`, and static-pair optimization off.
Run it with:

```sh
cargo test -p chialisp-oxide-backend-experiment --test runtime_calls --locked -- --nocapture
```

## What the experiment exercises

Each function receives `(compiler_function_table . argument_list)`. Arguments
form a proper list. The table holds quoted, finite function bodies; their code
refers to other table entries through paths. This creates self and mutual calls
without cyclic arena references or compile-time unrolling. The bootstrap
constructs the table itself, so the raw environment cannot supply a callee.

The test's teaching function follows the right child of each pair and returns
the final atom, preserving its complete bytes. One configuration calls itself;
another alternates between two functions. Both optimizer settings run at depths
0, 1, 2, 8, 128, and 2,048: 24 successful VM executions. The leaf is the arbitrary
atom `0x000001ff`, and every intervening left child is a different atom. The code
is compiled once per configuration and remains compact regardless of input
depth. Two additional VM executions deliberately call themselves with the same
argument and must produce `CostExceeded` at cost limit 100,000. No native Rust
function deliberately recurses forever.

The original isolated probe on 2026-10-16 measured:

- Self recursion: 117 bytes without optimization, 109 with it; depth 2,048 costs
  1,436,397 and 1,381,101 respectively.
- Mutual recursion: 169 and 157 bytes; depth 2,048 costs 1,440,564 and 1,385,268.

These are measurements of this LIR experiment, not canonical module identities
or a performance guarantee. The durable test prints fresh sizes and costs.

Rue's pinned implementation already lowers captured function environments and
self references into finite quoted code plus runtime application, and declines
to inline recursive functions. Its
[function lowering](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-hir/src/lower.rs#L143-L176),
[call lowering](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-hir/src/lower.rs#L429-L454),
and [Run/Closure code generation](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-lir/src/codegen.rs#L48-L92)
are primary implementation references. The experiment needs `Run`, not a new
first-class closure feature.

## Proposed version-4 owned IR

A future version-4 envelope can retain the existing environment policy and
entry-point fields while adding typed function definitions. Existing programs
should retain their version-1/2/3 wire formats. Older readers must reject the new
envelope rather than ignore its function table.

Proposed data shapes, not implemented APIs:

```text
Function { id, name, parameters, result, body }
Call     { function_id, arguments }
Let      { local_id, value, body }
Local    { local_id }
```

Predeclare reachable Rust function identities and signatures, then lower each
supported MIR body once in its own argument scope. Direct calls reference
validated function IDs; names are diagnostic labels, not intrinsic identities.
Preserve current restrictions on SDK dependencies, lifetime-only generics,
immutable references, unsafe code, destructors, and supported MIR operations.

`Let` evaluates its value once before its body, even if the body does not use
it. `Local` resolves only within its lexical scope; the initializer cannot read
its own new binding. Call arguments evaluate once, left to right, before the
callee runs. These bindings are necessary: the current expression substitution
and failure sequences could duplicate recursive work. Rust specifies
[left-to-right operand evaluation](https://doc.rust-lang.org/reference/expressions.html#evaluation-order-of-operands),
whereas clvmr schedules operand evaluations on a stack. A naive CLVM argument
list alone does not enforce the desired order when arguments may diverge.

A possible extended frame is `(function_table argument_list . lexical_values)`.
Each call starts with fresh lexical values; a binding applies its continuation
to an extended frame. The table remains compiler-generated and inaccessible to
source arguments. This extended binding convention is a proposal; the durable
probe tests only the simpler table/argument frame.

Validate all IDs, arities, argument/result types, lexical scopes, and aggregate
size budgets before lowering. Bound compilation by function count, body nodes,
work, and serialized expansion rather than trying to expand a runtime cycle.
Recursive calls remain potentially failing and diverging; a recursive component
must not acquire optimistic totality merely because its bodies are well typed.
The existing source-CFG loop rejection can remain until loop semantics receive
their own implementation.

The current tree-guard optimization also needs a divergence audit before calls
are added. Its successful-evaluation facts are sound for today's finite, pure
expression IR, which evaluates to a value or rejection. They are insufficient
for future expressions that may diverge. For example, this proposed expression
must retain its outer guard:

```text
If(IsPair(arg), Sequence(spin(), First(arg)), Reject)
```

For an atom input, the guarded form rejects immediately. Removing the guard
would enter `spin()` before reaching the checked projection and could diverge
until VM cost exhaustion. A future analysis must track evaluation before
possible divergence or conservatively block such rewrites. This is a required
obligation for adding calls, not a defect in the currently accepted finite IR.

The default strict ABI still validates the complete external argument list,
including unused parameters. Raw exports still require one logical tree;
ignored exports remain nullary. Runtime helpers introduce no solution-selected
`apply` operation and no new public environment policy.

## First source witnesses and remaining obligations

A first teaching witness can use the existing immutable borrowed tree type:

```rust
fn rightmost<'a>(tree: &'a Tree<'a>) -> &'a Tree<'a> {
    match tree {
        Tree::Pair(_, rest) => rightmost(rest),
        Tree::Atom(_) => tree,
    }
}
```

This Rust example illustrates the proposed feature; the current frontend rejects
its recursive call. It returns a borrowed input subtree and needs no fixed-width
number adapter or allocating output type.

The closest published follow-up is Chia gaming's
[`last_inner` helper](https://github.com/Chia-Network/chia-gaming/blob/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/clsp/std/last.clinc#L8-L13).
It recursively follows a list tail and returns its final element. An exact port
also needs narrow `Tree::Atom([])` matching, potentially recognizing validated
internal MIR `Len == 0` as an atom-empty predicate. That need not expose general
`usize` results or slicing. Nonempty `0x00` is truthy, not nil; an improper
nonempty atom tail must eventually reject. This is a published helper, not an
entire puzzle. Any module wrapper must preserve its original complete
argument/environment behavior before claiming entry-point equivalence.

Reverse and append create accumulating trees. The current borrowed `Tree`
cannot safely return references to stack-local accumulation nodes. They require
a separate arena/owned-value or other genuine Rust ownership design; adding
runtime calls alone does not solve that problem.

Before frontend support is claimed, implement and validate acyclic runtime calls
and bindings, then recursive call graphs, then the additional empty-atom pattern
and published helper. Required evidence includes native Rust, independent
Chialisp and CLVM comparisons; exact padded/improper-tree outputs; unused failing
or diverging calls; argument evaluation order; branch laziness; non-tail and
mutual calls; malformed IR rejection; and unchanged existing ABI validation.
Mutations should break callee selection, required evaluation, or the base case
and be detected.

Execution depth is input dependent, but resources are finite. The pinned
[VM implementation](https://github.com/Chia-Network/clvm_rs/blob/78feb9b2b0f9c2a4742af672d5fd43d55a559b23/src/run_program.rs#L538-L570)
enforces cost limits and explicit VM stack limits. Native Rust additionally uses
its native call stack; native and VM resource exhaustion differ. Keep native
recursive comparisons within a safe tested depth and test deeper VM execution
separately. The 24 finite executions and two cost controls are neither a
termination proof for arbitrary Rust recursion nor a source-to-CLVM preservation
proof. See [verification scope](verification.md).
