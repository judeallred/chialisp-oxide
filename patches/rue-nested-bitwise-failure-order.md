# Nested bitwise reductions must retain their integer checks

Affected upstream revision: `245c107a8e795229ea08097142f005a4c0d91075`.
Upstream report: <https://github.com/xch-dev/rue/issues/112>.
The [standalone regression and fix](rue-nested-bitwise-failure-order.patch)
applies to a clean Rue checkout of that revision, without any of the four
previous local patches.

The optimizer flattened nested `Logand` and `Logior` reductions. An inner
reduction checks that its evaluated arguments are integers before returning to
an outer operation. Flattening deferred that validation until every outer
operand expression had evaluated. A malformed value could therefore change an
immediate VM-semantic rejection into exhaustion of the execution cost budget.

The actual Rue frontend reaches the rewrite with this valid source:

```rue
fn main(...value: Int) -> Int {
    spin() & (value & 1)
}

fn spin() -> Int {
    spin()
}
```

Replacing both `&` operators with `|` reproduces the second case. The raw
whole-environment input `(1 . 2)` deliberately violates the entrypoint's `Int`
annotation. This is a malformed-external-input observation, **not** a demonstrated
mismatch on well-typed inputs, a bypass of an explicit source type guard, or a
vulnerability in the Rust compiler's strict typed ABI. No deployed puzzle or
funds exploit was assessed.

With clvmr **0.21.0**, `ChiaDialect::new(MEMPOOL_MODE)`, and a **1,000,000** cost
limit, both original source programs produce:

- Optimization off: actual `EvalErr::InvalidOpArg`, classified as
  program/`invalid_op_arg`; 55 serialized bytes.
- Optimization on before the fix: actual `EvalErr::CostExceeded`, classified as
  resource/`cost_exceeded`; 51 serialized bytes.
- Optimization on after the fix: `EvalErr::InvalidOpArg`; 55 serialized bytes.

The frontend settings keep `auto_inline=true`, `optimize_static_pairs=false`,
and `debug_symbols=false`, changing only `optimize_lir`. Explicitly guarding an
`Any` input with `assert value is Int;` raises in both modes. Well-typed atom
inputs reach the recursive operand and exhaust the cost budget in both modes.
Resource exhaustion is inconclusive; it does not confirm an abstract rejection.

A minimal public-LIR witness is:

```text
Logand([Run(Path(3), Path(3)), Logand([Path(2), Atom([1])])])
```

With environment `((1 . 2) . (a 1 1))`, the original CLVM is:

```clvm
(logand (a 3 3) (logand 2 (q . 1)))
```

CLVM evaluates operand expressions rightmost first. The inner reduction rejects
before the self-applying sibling executes. Flattening removes that intermediate
integer check. Reversing the operands must retain the opposite control: the
recursive operand reaches the cost limit first in both modes.

The conservative fix removes only the two flattening arms. Literal folding
remains enabled, and all-literal nested reductions can still fold before the
outer optimizer runs. No speculative totality assumption or caller guard is
introduced.

## Executed upstream regression evidence

Tests were added before changing the production source in an isolated clean
checkout. The LIR regression first had two intended failures and three passing
controls; the actual frontend regression first had two intended failures and
two passing controls. With the fix, all nine new tests pass, covering 120 VM
executions. The full `rue-lir` and `rue-compiler` test run passes **52 tests**:
43 existing LIR unit tests and the nine additions. The patch passes
`git apply --check` in a separate clean checkout.

```sh
git apply /path/to/rue-nested-bitwise-failure-order.patch
cargo test -p rue-lir -p rue-compiler --test nested_bitwise_failure_order
cargo test -p rue-lir -p rue-compiler
```

The original upstream Cargo.lock was not executed: offline resolution stopped
before compilation at an uncached `rustcrypto-ff` dependency selected through
`k256 0.14.0-rc.7`. The isolated checkout's lock was regenerated offline and then
held fixed for all test-first and post-fix runs. The proposed patch changes no
manifest or lockfile. Relevant executed versions were clvmr 0.21.0
(checksum `9e13f0990d3f7f4e88d8f8a49a0082ecdd42de4f2e5206d571c02fac9519c649`),
clvm-traits 0.51.0, num-bigint 0.4.8, malachite-bigint 0.9.2 and id-arena 2.3.0.

- Original upstream lock SHA256:
  `7a4b24d2703b4ad91b484391195462c1c5b0a22d5fa779c8f32eb7e617b7c17b`.
- Executed scratch lock SHA256:
  `6ba2dc237c6434970062491d29bb82842bb22351c7ef7500f94526fd83b986bf`.

That resolution changed 176 removed and 162 added package/version/source records
across the upstream workspace. The full difference record and failing/passing
logs were retained with the isolated reproduction. This disclosure does not
claim the original upstream dependency lock was tested.

## Repository regressions

The vendor copy retains the public-LIR regression, while
`crates/chialisp-oxide-rue/tests/nested_bitwise_failure_order.rs` exercises the
actual `compile_file` API with typed VM failures, both source operators,
malformed pairs, explicit guards and valid padded/signed atoms. Its cost-limit
controls require exactly resource/`cost_exceeded`; other errors cannot masquerade
as the intended rejection or resource observation.

```sh
cargo test --locked -p rue-lir@0.11.0 --test nested_bitwise_failure_order
cargo test --locked -p chialisp-oxide-rue --test nested_bitwise_failure_order
```
