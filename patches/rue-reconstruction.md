# Pair reconstruction must preserve projection failure

Affected upstream revision: `245c107a8e795229ea08097142f005a4c0d91075`.
The [regression and fix](rue-reconstruction-preserves-failure.patch) apply to
`crates/rue-lir` in that Rue checkout.
Upstream report: [xch-dev/rue#110](https://github.com/xch-dev/rue/issues/110).

The optimizer rewrote matching child paths into their parent:

```text
Cons(Path(first_path(parent)), Path(rest_path(parent))) -> Path(parent)
```

This loses the requirement that the selected parent is a pair. In particular,
`Cons(First(Path(1)), Rest(Path(1)))` rejects any atom environment without LIR
optimization. Optimization previously emitted the identity program `01`, which
accepted and returned that atom. `()` therefore changed from rejection to
acceptance. Pairs retained the same value in both configurations.

The public LIR reproduction runs in clvmr 0.21.0 with
`ChiaDialect::new(MEMPOOL_MODE)`, cost limit 1,000,000, and static-pair
optimization off. The regression checks parent paths 1, 2, and 3, both explicit
`First`/`Rest` expressions and precomputed child paths, five atom encodings and
four pair values, and LIR optimization off/on: 108 executions. Pair controls
check exact serialized values, including padded atoms and improper tails.
Before the fix the first optimized atom control fails; after it all pass.

This source also reaches the affected LIR through Rue's normal source compiler:

```rue
fn main(...value: (Any, Any)) -> (Any, Any) {
    (value.first, value.rest)
}
```

Atom environments are malformed inputs for this source's tuple parameter.
This witness establishes a change in external failure behavior, not a claim
that well-typed Rue pair values produce different results. The same partial
operation can be expressed directly in the project's validated raw-tree IR.
No deployed puzzle or funds exploit was assessed.

The conservative fix keeps the `Cons` and its child paths. Removing the checks
would require independent proof that the selected parent is a pair.

```sh
git apply /path/to/rue-reconstruction-preserves-failure.patch
cargo test -p rue-lir --test reconstruction_preserves_failure
```

For this repository's vendor directory, use:

```sh
git apply -p3 --directory=vendor/rue-lir patches/rue-reconstruction-preserves-failure.patch
```
