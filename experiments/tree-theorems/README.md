# Finite-tree algorithm theorems

This isolated experiment models recursive prefixing over finite immutable trees.
It has no Mathlib or other third-party dependencies. The pinned toolchain is
Lean 4.35.0. The source is checked with warnings treated as errors, and every
public theorem prints its axiom dependencies.

`TreeTheorems.lean` separates the direct recursive algorithm from a specification
that decodes a proper list and folds its elements onto an arbitrary final tree.
The theorem statements cover all finite trees and lists, not a bounded sample:

- Decoder correctness against a proper-list encoding.
- Direct prefix equality with the independent decoding/folding specification.
- Exact failure when the left tree is not a proper list.
- Preservation of the complete arbitrary final tail at the prefix boundary.
- The existing last-element algorithm against `List.getLast?`, including empty
  input and nonempty atom-tail rejection.
- A separate exact two-argument environment decoder, including rejection of
  additional arguments and any nonempty terminating atom (including padded nil).

The direct definitions are manually authored algorithm models. `lastInner`
models the structural behavior of the existing
[`last_inner` adapter](../../puzzles/last-list-element/reference.clsp); this file
does not extract or verify that source. The `strictTwo` theorems concern the
explicit strict ABI modeled here, not an assertion that an upstream Chialisp
helper or a loose argument-pattern module rejects extra arguments.

The model does not import Rust, MIR, the project IR, CLVM, or allocator behavior.
No theorem establishes that their implementations preserve this model, predicts
their resource use, or proves absence of security defects. Rust ownership and
drop erasure are additional frontend obligations. Recursive runtime IR remains
outside the existing SMT proof models.

With the pinned distribution's `bin` directory on `PATH`, run:

```sh
lean --version
lake build
lean -E warning --json -t0 TreeTheorems.lean
```

The final command checks imported modules again at trust level zero and prints
the axiom dependencies of every public theorem. No
`sorry`, `admit`, custom axiom, or native proof evaluation is used. Inspect the
reported axioms and the theorem statements, following Lean's
[proof-validation guidance](https://lean-lang.org/doc/reference/latest/ValidatingProofs/).
This experiment has not performed independent checker/comparator validation;
that stronger validation remains separate from ordinary kernel checking.

The initial pinned-toolchain check accepted all 21 public theorems, including
rechecking imports with `-t0`. Their printed dependency sets were empty or
contained only Lean's built-in `propext`. Algorithm-corruption controls keep the
specifications and theorem statements unchanged; a changed algorithm must still
prove those statements rather than substituting a weaker claim.

The experiment gate checks the 21 expected types independently and requires
exactly one axiom audit for each theorem. It generates 85 closed Lean equalities
from the current corpus: 69 `prefix-list` inputs and 16 `last-list-element`
inputs. Each input records native Rust and all five compiled builds: Rust and
Rue with optimization off/on, plus Chialisp. These equalities connect the model
to those finite recorded outputs. Optional VM replay executes all 425 retained
observations and compares exact results, costs, and typed failures. Resource
failures cannot stand in for rejection. The gate checks each fixture's actual
source, declared dependencies, exact helper span, module adapter, pinned
source/license/commit/tree snapshots, and current Cargo authoring hashes.

From the repository root, with Lean 4.35.0 available on `PATH`, run:

```sh
python3 experiments/tree-theorems/check-model.py --self-test
python3 experiments/tree-theorems/check-model.py --replay-vm \
  --cli target/debug/chialisp-oxide
python3 experiments/tree-theorems/check-model.py --self-test --replay-vm \
  --output experiments/tree-theorems/results.json
```

Use `--lean /path/to/lean` for an isolated toolchain. The gate checks the release
version and commit, rejects warnings and unreviewed axioms, and uses no native
proof evaluation. Its 38 corruption controls cover protocol errors, weakened
theorem statements, missing expected types, changed algorithms, and corrupted
finite execution/source/provenance evidence. It also
accepts comments containing proof-related keywords: keyword filtering is not
proof validation.

The finite Rust/Chialisp/Rue bridge is current corpus execution evidence. The
unbounded algorithm theorems remain separate from this finite bridge: neither
proves Rust/MIR/IR/CLVM preservation, recursive runtime-IR correctness, or
universal equivalence to a compiled artifact. An optional `--output` records
hashes and which replays actually ran; a recorded result never replaces
rerunning the checker.

To check the committed record without installing Lean or executing the VM:

```sh
python3 experiments/tree-theorems/check-model.py --check-recorded --self-test
```

This freshness-only mode recalculates current model, oracle, gate, corpus,
source/provenance, bridge and artifact hashes. It requires exactly the 21
reviewed theorem audits and allowed axioms, 85 equalities, all 425 recorded VM
replays, 10 artifacts and 38 full-run controls. Its 16 additional corruption
controls test the recorded-data checks. It does not execute the kernel or VM,
prove that a fabricated record represents a real run, or independently validate
the recorded tool binary hashes. The full command above remains necessary to
perform the actual proof checks and replays.
