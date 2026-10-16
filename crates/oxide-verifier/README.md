# IR-derived verification

`oxide-verifier` reads the typed `oxide-ir::Program` emitted by `oxide-driver`.
It generates SMT-LIB2 from the actual expression tree, checks nonvacuity, and
searches for a counterexample to a separate specification. Z3 runs as a process;
unknown, timeouts, solver errors, missing witnesses, and unsupported operations
fail closed. The CLI exits unsuccessfully for a counterexample or vacuous claim.

```sh
cargo run --locked -p oxide-verifier -- program.json specification.json
cargo run --locked -p oxide-verifier -- program.json specification.json --emit > queries.smt2
cargo run --locked -p oxide-verifier -- program.json --replay witness.json

# Real source extraction, proof generation, native execution, and mutation replay:
cargo build --workspace --locked
python3 scripts/check-ir-proofs.py --output corpus/proofs.json
```

For `fn puzzle(counter: u64) -> u64 { assert!(counter < 1000); counter + 1 }`, this
specification checks the accepted output range without restating the program's
guard in the translation:

```json
{
  "obligation": {
    "obligation": "on_success",
    "property": {
      "predicate": "all",
      "items": [
        {
          "predicate": "compare", "relation": "ge",
          "left": { "term": "output", "fields": [] },
          "right": { "term": "int", "value": "1" }
        },
        {
          "predicate": "compare", "relation": "le",
          "left": { "term": "output", "fields": [] },
          "right": { "term": "int", "value": "1000" }
        }
      ]
    }
  }
}
```

`assumptions` defaults to true. An acceptance-equivalence specification is:

```json
{
  "obligation": {
    "obligation": "acceptance_iff",
    "predicate": {
      "predicate": "compare", "relation": "lt",
      "left": { "term": "input", "argument": 0, "fields": [] },
      "right": { "term": "int", "value": "1000" }
    }
  }
}
```

`on_success` asks whether every successful execution satisfies the property.
`acceptance_iff` additionally checks both acceptance and rejection against an
input predicate. Input assumptions are optional and must mention inputs only.
Output terms are disallowed in assumptions and acceptance predicates. Tuple
projections use `fields`; an empty path selects the scalar itself. Specification
arithmetic uses mathematical integers so authors can independently describe a
checked result, sum, conservation law, or range. Integer strings are parsed as
decimal numbers before generating SMT, never interpolated as solver code.

Results contain typed input witnesses in JSON. A successful proof includes an
accepted input to establish nonvacuity. A failed property includes the actual
counterexample inputs; callers should replay them through native Rust and the
optimized and unoptimized CLVM artifacts. Tests demonstrate that replay for a
deliberately removed guard, and compare all i8 inputs for checked arithmetic.
The source-derived pipeline builds the real `oxide-puzzles` Cargo library and
selects its annotated `oxide_puzzles::bounded_counter::puzzle` export from a
fresh `cargo oxide` artifact bundle. The policy source remains
`puzzles/bounded-counter/puzzle.rs`; no copied policy wrapper is compiled.
The pipeline commits the extracted original/mutant IR and
SMT queries under `proofs/bounded-counter`, and writes generated website evidence
to `corpus/proofs.json`. Both optimization settings execute nonvacuity witnesses
and the mutation attack. Native Rust separately accepts the nonvacuity witnesses
and rejects the attack. Cost exhaustion fails replay instead of counting as a
policy rejection. Native witness replay calls the same Cargo policy library
through `oxide native`. Both reports fingerprint the declared local inputs in
`puzzles/authoring.json`, including the module graph, SDK, macros, configuration,
and lockfile; the proof run rechecks them before publishing. These fingerprints
do not cover the complete effective build environment or prove preservation.

## Claim scope

This is an **IR-model proof over well-typed input values**, using Z3's QF_NIA
logic. Signed/unsigned integer widths and checked add/subtract/multiply are
modeled. Bool, integers, tuples, unit, projections, strict sequences, lazy
conditionals, comparisons, and rejection are supported. Tuples evaluate every
field before projection, including fields whose values are discarded.

Byte values, byte operations, division, bitwise operations, loops, recursion,
hashing, signatures, and chain context are unsupported. Byte operations fail
explicitly even in unreachable branches. The model assumes ABI decoding has
already succeeded; it does not model malformed CLVM trees, noncanonical atoms,
decoding cost, or exhaustion of VM cost. Runtime adversarial tests cover those
separately. There is no proof here that rustc extraction, the SMT translator,
Rue optimization, or CLVM lowering preserves semantics. The differential tests
provide evidence for their tested domains. No Lean proof is included, and these
checks do not establish absence of all security breaches.
