# Canonical Boolean values after double negation

Upstream report: <https://github.com/xch-dev/rue/issues/113>.
Standalone clean-base [regression and fix](rue-canonical-boolean-values.patch).

Affected revision: `245c107a8e795229ea08097142f005a4c0d91075` (Rue 0.11.0). This standalone patch applies to a clean checkout and does not require any earlier local patches, including #112.

Value-level `Not(Not(x)) -> x` is sound only when x already returns a canonical Boolean. Arbitrary nonempty values are truthy, but they are not necessarily atom 1. `opt_not` currently unwraps both nots unconditionally after `opt_truthy` returns another Not. This affects ordinary comparisons as well as the public double-Not LIR node.

### Valid Rue source, valid integer input

```rue
fn main(...value: Int) -> Bool {
    value != 0
}
```

Execute with whole-environment input **2**:

- Optimization off: accepted canonical true, serialized **01**. CLVM: `(not (= 1 ()))`.
- Optimization on: accepted **02**. CLVM: `1` (the identity program).

A stronger source-level witness changes the Boolean answer:

```rue
fn main(...value: Int) -> Bool {
    (value != 0) == true
}
```

For input **2**, optimization off returns **01** while optimization on returns **80** (NIL/false). The source `(value & 6) != 0` similarly returns 01 off versus 02 on for inputs 2 and 3. Tuple Boolean fields are also affected.

These are canonical, well-typed integer inputs, not malformed values violating a parameter annotation. That distinguishes this report from #112. No deployed-puzzle/funds exploit was assessed.

Compilation holds `auto_inline=true`, `optimize_static_pairs=false`, and `debug_symbols=false` constant, changing only `optimize_lir`. Execution uses clvmr **0.21.0**, `ChiaDialect::new(MEMPOOL_MODE)`, with a **1,000,000** cost limit.

### Minimal public LIR

```text
Not(Not(Path(1)))
```

With environment atom 2, its unoptimized CLVM `(not (not 1))` returns atom 1. Optimization emits Path(1), returning atom 2. Wrapping the result in `Eq(..., Atom([1]))` changes true to false.

### Fix: distinguish values from truthiness

The patch allows value-level double-Not elision only for a producer known to return NIL or atom 1 when it succeeds: canonical literals, Not, Eq, Gt, GtBytes, Listp, All and Any. Unknown paths and integer masks are not classified as canonical Boolean producers. Returning a known producer still evaluates that producer and preserves its possible failure.

Separately, `opt_truthy(Not(Not(x)))` may use `opt_truthy(x)`, because that caller observes truthiness rather than exact value bytes. The original expression still evaluates once. This retains the useful condition-only simplification instead of globally disabling double-negation optimization.

The scope is Boolean normalization and the stated producer/error controls, not a proof of general arbitrary-LIR preservation. Other truthiness rewrite concerns, such as validation of malformed pair operands by Eq(x,NIL), are separate from this value-normalization fix.

### Test-first and post-fix results

The patch contains five public-LIR tests and five actual Rue frontend tests, plus six small Rue fixtures.

Before changing production source:

- `rue-lir --test canonical_boolean_values`: **3 pass, 2 fail**, specifically unknown returned values and numeric masks leaking noncanonical bytes.
- `rue-compiler --test canonical_boolean_values`: **1 pass, 4 fail**, specifically returned comparisons, comparison to true, masks, and tuple/nested Boolean uses. The condition-only control passes.

After the fix:

```sh
cargo test -p rue-lir -p rue-compiler --test canonical_boolean_values
cargo test -p rue-lir -p rue-compiler
```

All **10 new tests** pass. The full second command passes **53 tests total**: 43 existing LIR unit tests plus the ten additions. It also completes both crates' zero-test doc-test suites without failure. These tests cover 329 VM executions, including canonical signed integer inputs, nonminimal and arbitrary raw LIR values, exact returned bytes, comparison with true, mask results, tuple fields, eager Boolean combinations, safe elision for known producers, condition-only compiled-byte equality, and required Raise/InvalidOpArg failures in value/comparison/conditional contexts.

`git diff --check` and `git apply --check` in a separate clean checkout both pass. The new LIR failure-control tests match actual native `EvalErr::Raise` and `EvalErr::InvalidOpArg` variants; any other VM failure fails the test.

### Dependency reproducibility disclosure

The original upstream Cargo.lock was **not** executed. As in the isolated reproduction for #112, offline resolution of that lock stopped before compilation at an uncached `rustcrypto-ff` dependency selected through `k256 0.14.0-rc.7`. The execution used the same offline-regenerated scratch lock, held fixed for failing-before and passing-after runs. The proposed patch makes no manifest or lockfile changes.

The executed VM is registry clvmr **0.21.0**, crate checksum `9e13f0990d3f7f4e88d8f8a49a0082ecdd42de4f2e5206d571c02fac9519c649`. Other relevant resolved versions: clvm-traits 0.51.0, num-bigint 0.4.8, malachite-bigint 0.9.2, id-arena 2.3.0.

- Original upstream Cargo.lock SHA256: `7a4b24d2703b4ad91b484391195462c1c5b0a22d5fa779c8f32eb7e617b7c17b`.
- Executed scratch Cargo.lock SHA256: `6ba2dc237c6434970062491d29bb82842bb22351c7ef7500f94526fd83b986bf`.

The offline resolution differs by 176 removed and 162 added package/version/source records across the workspace; the complete local difference record and test logs are retained. These are disclosed dependency-resolution differences, not production source changes or a claim to have tested the original upstream lock.

## Repository regressions

The vendor copy retains the public-LIR regression. The actual `compile_file`
adapter is exercised by
`crates/chialisp-oxide-rue/tests/canonical_boolean_values.rs`, using the same six
Rue source fixtures, canonical signed inputs, exact serialized Boolean/tuple
outputs, equality with true, eager Boolean operations and condition-only uses.
Unexpected typed VM failures fail the test; they are not policy-rejection evidence.

```sh
cargo test --locked -p rue-lir@0.11.0 --test canonical_boolean_values
cargo test --locked -p chialisp-oxide-rue --test canonical_boolean_values
```
