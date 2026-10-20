# Optional output lists with standard Rust Option

**WIP prototype. Not ready for use with real funds or in production.** This
feature provides a restricted CLVM output encoding for ordinary Rust
`Option<T>`. It does not add general enum support, a variable-length collection
library, or a completed boomerang puzzle.

Start a puzzle in a normal Cargo library using the unpublished
`chialisp-oxide` facade and `#[chialisp_oxide::puzzle]`. Native `cargo check` and
`cargo test` keep ordinary Rust semantics. The additional
`cargo chialisp-oxide check` or `build` checks which reachable operations can be
compiled to CLVM. See the [authoring guide](https://judeallred.github.io/chialisp-oxide/reference/agent-guide/) for the pinned toolchain and
complete workflow.

## A missing value and a one-element list

For an admitted payload type `T`, the CLVM output encoding is:

- `None::<T>` becomes `()`.
- `Some(value)` becomes `(encoded_value)`.

The outer one-element list always remains present for `Some`. In particular,
`None::<()>` becomes `()`, whereas `Some(())` becomes `(())`. `Some(0_u8)` also
becomes `(())`, because a canonical integer zero is the empty atom. The declared
payload type distinguishes those Rust types; the encoding does not attach a
Rust type name to every value. The `None` and `Some` variants remain distinct
within their declared payload type.

Bounded nested outputs preserve each layer: `Some(None::<u8>)` becomes `(())`,
and `Some(Some(0_u8))` becomes `((()))`. Fixed byte payloads preserve their
bytes, including leading zero bytes. An optional borrowed `Tree` preserves the
complete payload tree, including improper tails, inside a proper outer list.

## Extend a list with an optional last element

The `tail` export in the actual
[Cargo integration test](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-driver/tests/optional_lists.rs)
uses this function:

```rust
use chialisp_oxide::Cons;

#[chialisp_oxide::puzzle]
pub fn tail(choose: bool, n: u8) -> Cons<u8, Option<u8>> {
    Cons { first: 7, rest: if choose { Some(n) } else { None } }
}
```

`Cons` prepends one element to its tail. For native inputs `false, 9`, the
returned Rust value has `first == 7` and `rest == None`; its CLVM encoding is
`(7)`. For `true, 9`, `rest` is `Some(9)` and the encoding is `(7 9)`.
The strict CLVM argument environments for these calls are `(() 9)` and `(1 9)`.

A tuple retains all its fields: `(7_u8, None::<u8>)` encodes `(7 ())`, not `(7)`.
Use `Cons` when the optional value should form the final list tail. Wrapping an
entire conditions tuple in `Some` would instead add a list around that tuple.

## Build named condition lists

A local assembly helper can prepend fixed conditions to an optional signature
condition. This helper is part of the same Cargo test fixture and is reached
through an annotated test wrapper. It illustrates construction and is not an
annotated coin puzzle or a validated spending policy:

```rust
use chialisp_oxide::{
    AggSigMe, Amount, AssertMyAmount, AssertSecondsAbsolute, Cons, CreateCoin,
    PuzzleHash, Timestamp, assert_before_seconds_absolute, assert_my_amount,
    assert_seconds_absolute, create_coin,
};

pub type DeadlineConditions = Cons<
    AssertSecondsAbsolute,
    Cons<AssertMyAmount, Cons<CreateCoin, Option<AggSigMe>>>,
>;

pub fn assemble_conditions(
    before_deadline: bool,
    deadline: Timestamp,
    amount: Amount,
    destination: PuzzleHash,
    authorization: Option<AggSigMe>,
) -> DeadlineConditions {
    let time_condition = if before_deadline {
        assert_before_seconds_absolute(deadline)
    } else {
        assert_seconds_absolute(deadline)
    };
    Cons {
        first: time_condition,
        rest: Cons {
            first: assert_my_amount(amount),
            rest: Cons {
                first: create_coin(destination, amount),
                rest: authorization,
            },
        },
    }
}
```

The caller constructs `Some(agg_sig_me(key, intent))` or `None` inside supported
Rust code. The helper returns three or four conditions without adding an
unused signature or padding condition. Both named time-condition result aliases
currently share the same tuple representation, so the chosen constructor
determines which time condition is emitted. Consensus checks the chain time;
`before_deadline` selects a condition and is not a claim about the current time.
The before condition requires that time to be strictly below the deadline;
the after condition permits equality. These are consensus timestamp checks,
not reads of the author's wall clock.

This helper deliberately does not decide who may spend or when authorization
is required. An actual exported policy must make those decisions and commit
its destinations, amount and deadline appropriately. Marking this helper itself
as an export would fail because its `authorization` parameter contains
`Option`. Only compiler-created optional values may be passed to internal
helpers. A complete reclaimable-payment tutorial also needs curried instances,
independent language implementations, adversarial vectors and actual ledger
spends. The assembly helper is checked as a function, not as that complete
puzzle. Its test wrapper's key bytes are opaque wire fixtures; the tests do not
verify BLS signatures or submit these conditions to consensus.

## Keep the supported boundary explicit

The compiler recognizes the actual standard `core::option::Option` and its
variants through rustc identities and the pinned core crate. Normal aliases and
`std::option::Option` reexports retain that identity. A local enum named
`Option`, a spoofed module path, or an unrelated SDK declaration does not gain
this encoding.

Supported operations are construction, ordinary whole-value assignment, moves,
copies where Rust permits them, and opaque forwarding through local
monomorphic helpers. A helper may accept and return an optional value built by
another checked function. Bounded nesting in admitted tuples, arrays and
`Cons` outputs is allowed, within the existing type-depth and work limits.

The current subset rejects:

- `Option` anywhere in an exported parameter, including nested tuple, array,
  transparent-wrapper and `Cons` fields. The exact typed environment ABI does
  not decode optional inputs.
- Matching or inspecting an optional value, discriminant or payload
  projections, and methods such as `is_some`, `unwrap` and `map`.
- References to optional values. An optional immutable borrowed `Tree` or
  admitted atom payload is a different supported shape.
- Owned-tree, `Box`, `Vec`, custom-drop and other unsupported payload types,
  even when the value being constructed is `None`. An empty variant does not
  authorize its otherwise unsupported payload type.
- Arbitrary enums, trait-based optional operations, and general generic
  functions or collection APIs.

Constant admission is narrow: only an **already evaluated, fieldless standard
`None`** constant is recognized. This is not general optional constant
evaluation. `Some` constants and other nontrivial optional constants remain
unsupported. Construct the optional value in a supported function, as in
`tail`, rather than relying on a named optional `const` or CTFE helper.

An initializer still runs before its value is wrapped or discarded.
`Some(checked_value())` cannot hide a failing assertion, overflow, hash or call.
An unused `Some` initializer retains those effects. A `Some` expression in an
unselected `if` arm does not evaluate its payload. Ordinary Rust checks remain
responsible for moves and lifetimes; the CLVM subset preserves the admitted
evaluations rather than treating `Option` as lazy execution.

## Wire format, optimization and evidence

The typed IR has an explicit `Optional` type and `OptionalNone`/`OptionalSome`
constructors. Programs with optional capability use version 6, including when
they also use version-5 `Cons` pairs. Capability discovery includes unused
function signatures, local type metadata and dead branches. Older envelopes
reject these types and nodes; existing programs without optional capability
retain their earlier formats. The output contains nil or a list, never a Rust
discriminant, niche layout or pointer representation.

Optional-capable programs currently skip the Rust IR refinement and inlining
passes. Their complete validated IR is retained conservatively. Selecting
optimization still controls the Rue backend and the reviewed decoder/runtime
paths; an optimization-on build is not a claim that optional Rust IR rewrites
have been implemented or proved.

The implementation's evidence has separate scopes:

- [Driver integration tests](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-driver/tests/optional_lists.rs)
  compare actual Cargo Rust functions and native results against independently
  compiled Chialisp, with both optimization modes and wire round trips. They
  also exercise rejected inputs, unsupported operations, identities, ownership
  and source-order controls. Both Rust examples on this page are bound exactly
  to that fixture's source. The condition helper is exercised with and without
  authorization and with both named time conditions; those are output-tree
  comparisons, not consensus authorization or timelock observations.
- [IR tests](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-ir/tests/optional.rs) exercise exact output
  trees, strict evaluation, capability versions, internal calls and bounded
  metadata validation independently of Rust extraction.
- [Verifier tests](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-verifier/tests/optional.rs) require
  every existing proof model to reject optional capability, including unused
  and dead occurrences. No optional-output proof model is currently admitted.
- Simulator tests for before/after time conditions check pinned consensus
  behavior. They do not turn these constructor tests into a complete boomerang
  spend or prove a policy's authorization and liveness.

These are finite regressions and model-boundary checks. Resource exhaustion is
inconclusive, not evidence that a malicious spend was correctly rejected.
Neither the feature nor its tests establish compiler preservation, complete
puzzle coverage or production readiness. See [verification.md](https://github.com/judeallred/chialisp-oxide/blob/main/docs/verification.md).
