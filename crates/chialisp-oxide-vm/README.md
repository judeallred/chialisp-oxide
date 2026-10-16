# Host-side VM failure protocol

This unpublished crate classifies the typed errors of **clvmr 0.21.0** for the
project's execution and verification harnesses. It is separate from the
`no_std` puzzle authoring SDK and adds no on-chain Rust operation.

Every VM error has a closed code and one of three kinds:

- **Program:** execution rejected under the pinned VM's semantics. This can be
  an explicit raise, a checked projection into an atom, or an invalid operator
  argument. It does not establish that the intended security check ran.
- **Resource:** cost, timeout, memory, atom/pair capacity, or VM stack exhaustion.
  The generic execution harness reports `inconclusive`; witness replay aborts.
  These failures cannot confirm an abstract policy-rejection claim.
- **Infrastructure:** codec errors and internal VM errors. These also remain
  inconclusive or abort the harness. Setup/serialization errors propagated
  before execution likewise cannot become successful rejection evidence.

`classify_failure` matches every pinned `EvalErr` variant without a fallback.
An upstream enum change therefore requires an explicit classification review.
The [closed protocol table](failure-protocol.json) is shared by Python and Node
evidence gates. A Rust contract test checks that it matches the typed wire
format. Deserialization rejects unknown codes, unknown fields, and a kind that
contradicts its code; callers cannot relabel `cost_exceeded` as `program`.

Some VM names need source-level interpretation: `InvalidAllocArg` currently
validates substring bounds and G1/G2 point encodings, so it is a program error,
rather than an allocation-capacity failure. `ShiftTooLarge` enforces the VM's
explicit shift-operand restriction. `OutOfMemory`, `TooManyAtoms` and
`TooManyPairs` are resource errors. Classifications describe this pinned VM;
they are not a theorem about arbitrary interpreters or Rust policies.

The backend keeps its diagnostic string for inspection and emits typed
`failure` metadata for every unsuccessful execution. Successful executions omit
that field. Its outcome is `accepted`, `rejected`, or `inconclusive`. The CLI
returns a nonzero exit status for both unsuccessful outcomes. Recorded corpus
and proof evidence requires conclusive execution; diagnostics are not parsed
to make that decision. Counter-policy proofs additionally require the precise
`raise` code when their witness must reach an assertion.

## Reproduce the controls

```sh
cargo test --locked -p chialisp-oxide-vm
cargo test --locked -p chialisp-oxide-backend-experiment --test differential
cargo test --locked -p chialisp-oxide-verifier
```

The VM tests use a bounded heap to force a real concatenation allocation
failure, virtual allocator reservations to exercise atom/pair capacity without
millions of allocations, and bounded VM self-application for cost/timeout
failures. The same finite programs also have successful allocation controls.
Large-stack errors are explicitly labeled classifier injections: the suite
does not exhaust the VM's twenty-million-entry stacks. No native Rust function
deliberately runs forever or exhausts its call stack.

These controls check execution classification, not universal termination,
compiler preservation, consensus security, or freedom from policy mistakes.
