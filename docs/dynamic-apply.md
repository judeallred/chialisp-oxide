# Dynamic application

**WIP prototype. Applying an untrusted program establishes no authorization,
termination, cost bound or compiler-preservation property.**

`chialisp_oxide::apply(program, environment)` is CLVM's `a` operator as an
ordinary safe Rust function over the borrowed `Tree`:

```rust
use chialisp_oxide::{Tree, apply};

#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(inner_puzzle: &'a Tree<'a>, inner_solution: &'a Tree<'a>) -> Tree<'static> {
    apply(inner_puzzle, inner_solution)
}
```

The result is an arbitrary tree. Both operands are evaluated once, in source
order, before the application. A failing applied program fails the caller; an
unused result is still evaluated; an application in an unchosen branch does not
run. Nesting is ordinary: the result of one application can be the program of
the next.

## Why the compiler needs implementation identity

A Rust function cannot run CLVM without an interpreter. The facade is `no_std`;
without the `vm` feature the reserved export is a bare `panic!()`, and with it
the function evaluates through the pinned VM in mempool mode. The compiler does
not trust either body because of its name or package. As for the
[SHA-256 bridge](hashing.md), a candidate is a public root export named `apply`
of the `chialisp_oxide` crate, admitted only when:

- it is an ordinary non-const function with lifetime-only generics, exactly two
  immutable references to the structurally admitted borrowed tree and that same
  tree by value as its result;
- the actual call's operand and result types equal the reviewed signature; and
- the canonical fingerprint of its typed-MIR closure, profile and target equals
  the pin reviewed for that host.

The fingerprinted body is the `vm`-less panic: one call of the installed core
`panic` lang item with a `&'static str`. Admitting that leaf, string-slice
constants and the structurally checked tree enum are the only additions to the
canonical encoder; the SHA kernel's fingerprint is unchanged. A relocated copy
of the reviewed body is admitted; a changed body, a looping body or a body
using unreviewed constructs fails closed with an explicit diagnostic. An
ordinary local function named `apply` keeps its real body. Cargo extraction
builds the puzzles library without `vm`, so the compiler always sees the panic
body; building it with `vm` on changes the fingerprint and is rejected.

## IR, lowering and optimization

`Expr::Apply { program, environment }` has result type `Tree` and requires both
operands to be arbitrary trees. It is a version-4 runtime node: the acyclic
expression path refuses it, the driver selects the strict runtime function
table when a call is reachable, and the totality analysis treats the call as
non-total so unused results survive. The backend binds both operands strictly
and emits `(a program environment)` through Rue LIR's `Run` node. The optimizer
walks the node like a two-operand expression, never folds it, never moves it
past divergence and derives no facts from its result. Legacy version-3 wire
formats reject the node.

Every proof API rejects programs containing an application, including dead
branches, unused bindings and unused function-table entries. No model
substitutes an uninterpreted or deterministic function for it.

## Native oracle contract

With the `vm` feature the native function encodes both trees iteratively,
runs the pinned VM with a fixed native ceiling of ten billion cost units and a
node bound, and decodes the result into leaked owned nodes. The VM charges the
applied program against the outer puzzle's budget; a native oracle has no such
budget, so the native ceiling is deliberately generous. Exceeding it, or a host
allocation limit, panics with a `ResourceExhausted` payload that the corpus
oracle reports as a host error (inconclusive), never as policy rejection. A
program failure panics ordinarily and is a rejection, matching the VM.

The corpus enables `vm` for the oracle and as a dev-dependency of the puzzles
library; `cargo chialisp-oxide build -p chialisp-oxide-puzzles` never sees it.

## What the first witness establishes

`puzzles/delegated-puzzle` is an original teaching fixture with a strict
two-argument ABI, an authored Chialisp comparison with explicit shape checks
and a Rue translation that applies a typed function value with a spread
solution and guards the remaining list shape. Sixteen named cases agree across
native Rust and the five CLVM builds: quoted constants, environment paths,
arithmetic over solution fields, pair construction, nil and padded results,
raising programs, paths into atoms and every malformed argument shape.

This does not implement currying, inner-puzzle hash commitments, signature
wrapping or any canonical delegated-puzzle module. Those need their own
witnesses, and the three remaining chialisp.com inner-puzzle and
condition-morphing programs additionally need the still-unpinned include
libraries or integer and list operations the compiler does not provide.

## Closing the identity gap

`puzzles/inner-puzzle` adds the missing commitment: a 32-byte inner puzzle
hash as the first (curried) argument, a tree hash of the supplied inner
program through the SHA-256 bridge, and `apply` only on a match. The Chialisp
comparison includes the pinned upstream `sha256tree.clib` behind a strict
adapter and the Rue comparison uses its `tree_hash` builtin. Named cases cover
matching and mismatched commitments, raising inner programs and every
malformed shape; the manifest's curried examples run two host-curried
commitments in all five builds. It still authorizes no spender.
