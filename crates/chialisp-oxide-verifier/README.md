# IR-derived verification

`chialisp-oxide-verifier` reads the typed `chialisp-oxide-ir::Program` emitted by `chialisp-oxide-driver`.
It generates SMT-LIB2 from the actual expression tree, checks nonvacuity, and
searches for a counterexample to a separate specification. Z3 runs as a process;
unknown, timeouts, solver errors, missing witnesses, and unsupported operations
fail closed. The CLI exits unsuccessfully for a counterexample or vacuous claim.

```sh
cargo run --locked -p chialisp-oxide-verifier -- program.json specification.json
cargo run --locked -p chialisp-oxide-verifier -- program.json specification.json --emit > queries.smt2
cargo run --locked -p chialisp-oxide-verifier -- program.json --replay witness.json

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
The source-derived pipeline builds the real `chialisp-oxide-puzzles` Cargo library and
selects its annotated `chialisp_oxide_puzzles::bounded_counter::puzzle` export from a
fresh `cargo chialisp-oxide` artifact bundle. The policy source remains
`puzzles/bounded-counter/puzzle.rs`; no copied policy wrapper is compiled.
The pipeline commits the extracted original/mutant IR and
SMT queries under `proofs/bounded-counter`, and writes generated website evidence
to `corpus/proofs.json`. Both optimization settings execute nonvacuity witnesses
and the mutation attack. Native Rust separately accepts the nonvacuity witnesses
and rejects the attack. Cost exhaustion fails replay instead of counting as a
policy rejection. Native witness replay calls the same Cargo policy library
through `chialisp-oxide native`. Both reports fingerprint the declared local inputs in
`puzzles/authoring.json`, including the module graph, SDK, macros, configuration,
and lockfile; the proof run rechecks them before publishing. These fingerprints
do not cover the complete effective build environment or prove preservation.

## Separate structural tree checker

The raw-tree checker uses a different symbolic domain: the recursive datatype
`Tree = Atom(Seq(BitVec8)) | Pair(Tree, Tree)`. Atoms preserve every byte, including
empty values and numeric padding. The domain includes all finite trees and byte
sequences; it is not a bounded numeric approximation. This checker requires one
`Tree` parameter, an explicit raw-environment ABI and a `Tree` result. It models
tree shape tests, checked first/rest/atom projections, atom/pair construction,
booleans, lazy branches and strict sequences. Other operations fail closed,
including operations in an unchosen branch. The scalar checker independently
rejects tree/atom types and tree expressions instead of silently treating them
as numbers or typed argument lists.

An independently authored p2_conditions specification is:

```json
{
  "acceptance": "pair",
  "output": {
    "term": "pair",
    "first": { "term": "atom", "hex": "01" },
    "rest": { "term": "first", "value": { "term": "input" } }
  }
}
```

This says an atom environment rejects; a pair `(conditions . arbitrary_tail)`
returns the quoted program `(1 . conditions)`, preserving the raw payload. It
does not interpret the payload as valid spend conditions or authenticate a
spend. Atom and pair cases each have a satisfiable **domain nonvacuity** query
and a counterexample query. Atom-case nonvacuity intentionally demonstrates a
rejected input, not a successful execution. Both counterexample queries must
be unsatisfiable before the structural model is reported proven. A satisfiable
counterexample produces a concrete raw-tree witness. Stable candidate witnesses
are checked by the solver only after its general, unrestricted SAT result;
they do not replace or bound the proof obligation.

```sh
cargo run --locked -p chialisp-oxide-verifier -- program.json --tree tree.spec.json
cargo run --locked -p chialisp-oxide-verifier -- program.json --tree tree.spec.json --emit
cargo run --locked -p chialisp-oxide-verifier -- program.json --tree tree.spec.json --emit-json
cargo run --locked -p chialisp-oxide-verifier -- program.json --tree-replay tree.witness.json
cargo run --locked -p chialisp-oxide-verifier -- program.json --optimize-ir > optimized.json
cargo run --locked -p chialisp-oxide-verifier -- program.json --tree-equivalent optimized.json
cargo run --locked -p chialisp-oxide-verifier -- program.json --artifacts

# Source-derived Cargo export, independent canonical compilation and mutations:
python3 scripts/check-tree-proofs.py --output corpus/tree-proofs.json
```

Tree witnesses use `{"environment":{"kind":"atom","hex":"0001"}}` or recursive
`{"kind":"pair","first":...,"rest":...}` values. Raw replay supplies the entire
environment directly, without a typed one-argument-list wrapper. Atom hex is
validated before generating SMT or CLVM. Witness replay has explicit resource
limits and treats cost exhaustion as an error. Those practical replay limits
do not narrow the structural model's quantified tree domain.

`chialisp-oxide-tree-reference` separately compiles trusted Chialisp source and
executes at most 32 witness files with a fixed ten-million-cost limit. Its
artifact includes serialized bytes and a computed tree hash. The source-derived
pipeline requires the exact published p2_conditions artifact before comparing
Rust/native/CLVM witness results. Stored IR normalizes only repository prefixes
inside diagnostic `Reject.message` metadata; operators, values, types and policy
structure remain those from the Cargo export.

The pipeline separately stores the real IR optimizer's result and checks it
against both the independent canonical specification and the original IR. The
equivalence query compares rejection and successful tree outputs over all finite
raw environments, using the symbolic execution semantics rather than repeating
the optimizer's required-projection analysis. This proves these particular IR
expressions equivalent in the model; it is not a general proof of the optimizer
implementation. Rejection diagnostics and execution cost may differ.

Artifact evidence records four compilation paths. `compile(original, false)`
bypasses both the owned IR pass and Rue LIR optimization;
`compile(original, true)` applies both. Compiling the explicitly rewritten IR
with either setting independently exposes its emitted bytes and tree hash. The
script requires canonical module identity for the two paths with Rue LIR
optimization enabled. The rewritten path with optimization disabled retains
ordinary projection code and is measured separately. Module identity comes from
the actual artifacts, not the semantic proof. A synthetic control removes a guard whose
kept branch has no required pair projection. The solver must find an atom
counterexample, and both VM modes must reproduce its changed acceptance. This
control is labeled synthetic rather than attributed to the Cargo puzzle.

The tree result is a **structural IR-model proof**. It does not prove rustc
extraction, the SMT translation, CLVM opcode/serialization semantics, Rue
optimization, canonical module identity, VM resource behavior or consensus
security. Source-derived artifacts and actual native/VM witness replays connect
the pipeline for those executions. There is no general compiler-preservation
theorem or Lean proof here.

## Scalar claim scope

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
