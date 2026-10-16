# Quoted execution must retain its nil environment

Affected upstream revision: `245c107a8e795229ea08097142f005a4c0d91075`.
The [standalone regression and fix](rue-quoted-run-nil.patch) apply to
`crates/rue-lir` in that Rue checkout.
Upstream report: <https://github.com/xch-dev/rue/issues/111>.

The optimizer discarded an explicit nil environment when a program was quoted:

```text
Run(Quote(program), Atom(nil)) -> program
```

The right-hand side runs in the outer environment instead of nil. For example,
`Run(Quote(Path(1)), Atom(nil))` must return nil regardless of the outer input.
Before the fix, LIR optimization changed it into the identity program `01`,
returning the outer input. `Run(Quote(Path(2)), Atom(nil))` must reject because
nil has no first element. Optimization changed it into `02`, which accepted an
outer pair and returned its first element. This changes both values and failure
behavior.

The normal Rue source frontend also reaches this rewrite:

```rue
fn main(...outer: Any) -> Any {
    let identity = fn(...value: Any) => value;
    identity(...nil)
}
```

Here nil is a valid `Any` argument. Before the fix, optimization off returned nil
for outer inputs `()`, `1`, and `(1 . 2)`. With `auto_inline=true`,
`optimize_static_pairs=false`, and only `optimize_lir` switched on, the frontend
emitted `01` and returned the outer value instead. This witness does not depend
on malformed values violating a tuple parameter's declared type.

After rebuilding the actual Rue frontend with the fix, this source returns nil
for all seven regression environments with optimization off/on (14 VM runs).
Its optimized program is `ff02ffff0101ff8080`, retaining application to nil.

The regression executes actual generated CLVM in clvmr 0.21.0 with
`ChiaDialect::new(MEMPOOL_MODE)`, a 1,000,000 cost limit, and static-pair
optimization off. It checks seven outer environments, nil-bound identity and
four projection paths, the valid same-environment rule, and padded-atom constant
programs: 112 VM executions across LIR optimization off/on. Before the fix, two
tests fail while the control test passes; after the fix all three tests pass.

The conservative fix removes only the nil-environment cancellation. Running a
quoted program in the current environment still permits the existing
`Run(Quote(program), Path(1)) -> program` rewrite. No deployed puzzle or funds
exploit was assessed.

```sh
git apply /path/to/rue-quoted-run-nil.patch
cargo test -p rue-lir --test run_nil_environment
```

For this repository's vendor directory, apply the standalone patch with:

```sh
git apply -p3 --directory=vendor/rue-lir patches/rue-quoted-run-nil.patch
cargo test --locked -p rue-lir@0.11.0 --test run_nil_environment
```
