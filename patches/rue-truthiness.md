# Rue truthiness must preserve failing pair evaluation

Upstream report: [xch-dev/rue#109](https://github.com/xch-dev/rue/issues/109).
Affected upstream revision: `245c107a8e795229ea08097142f005a4c0d91075`.

The [patch](rue-truthiness-preserves-failure.patch) includes the fix and an
executable LIR regression covering `If`, `If(Listp(...))`, `Not`, `All`, and
`Any`, with failure in either pair field and optimization both on and off.
All 20 executions must reject. Before the fix, the first optimized case
incorrectly accepts. After the fix, all cases reject.

This typed Rue source reaches the same bug with the empty solution:

```rue
fn main() -> Int {
    let values: List<Int> = [1 / 0];
    if values is nil { 0 } else { 1 }
}
```

Measured through `FileTree::compile_file`, `tree.main`, and clvmr 0.21.0 with
`ChiaDialect::new(MEMPOOL_MODE)`, cost limit 1,000,000, static-pair optimization
off, and debug symbols off:

- Automatic inlining off: both LIR settings reject `DivisionByZero`.
- Automatic inlining on, LIR optimization off: rejects `DivisionByZero`.
- Automatic inlining on, LIR optimization on: incorrectly accepts and returns 1.

With the patch, all four configurations reject `DivisionByZero`.

`opt_truthy` previously inferred `Cons` was always true and discarded its
evaluation. CLVM first evaluates both fields, either of which can fail. The
conservative fix returns the original expression, preserving that evaluation.
It may retain computations that a future effect analysis could safely remove.

To apply the patch to a matching Rue checkout:

```sh
git apply /path/to/rue-truthiness-preserves-failure.patch
cargo test -p rue-lir --test truthiness_preserves_failure
```

To apply it to this repository's corresponding vendored crate:

```sh
git apply -p3 --directory=vendor/rue-lir patches/rue-truthiness-preserves-failure.patch
```

The report establishes a compiler acceptance/rejection discrepancy. No deployed
puzzle or funds exploit was assessed.
